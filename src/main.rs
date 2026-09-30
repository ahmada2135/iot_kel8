use esp_idf_hal::{
    delay::FreeRtos,
    gpio::*,
    i2c::{I2cConfig, I2cDriver},
    peripherals::Peripherals,
    units::*,
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    mqtt::client::{EspMqttClient, MqttClientConfiguration, QoS},
    nvs::EspDefaultNvsPartition,
    systime::EspSystemTime,
    wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use dht_sensor::{dht22, DhtReading};

// ─── Wi-Fi credentials ────────────────────────────────────
const WIFI_SSID: &str = "Tiga Bersaudara";
const WIFI_PASS: &str = "kosbununung";

// ─── ThingsBoard MQTT ─────────────────────────────────────
const TB_HOST: &str = "mqtt://eu.thingsboard.cloud";
const TB_PORT: u16 = 1883;
const TB_TOKEN: &str = "dcxfksvtvzdclffa6to5"; // Ganti dengan token dari ThingsBoard

const TB_TOPIC_TELEMETRY: &str = "v1/devices/me/telemetry";
const TB_TOPIC_ATTRIBUTES: &str = "v1/devices/me/attributes";

// ─── ADS1115 I2C addresses ────────────────────────────────
const ADS1_ADDR: u8 = 0x48; // Sensor gas 1-4  (MQ6, MQ135, MQ3, MQ7)
const ADS2_ADDR: u8 = 0x49; // Sensor gas 5-8  (TGS2611, TGS2602, TGS2600, TGS2620)

// ─── ADS1115 register constants ───────────────────────────
const ADS_REG_CONVERT: u8 = 0x00;
const ADS_REG_CONFIG: u8 = 0x01;

/// Build ADS1115 config word (PGA ±6.144 V, single-shot, 128 SPS)
fn ads_config(channel: u8) -> [u8; 3] {
    let mux: u16 = match channel {
        0 => 0b100, // AIN0 vs GND
        1 => 0b101,
        2 => 0b110,
        3 => 0b111,
        _ => 0b100,
    };
    // MSB: OS=1 (start conversion), MUX[2:0], PGA=000 (6.144V), MODE=1 (single-shot)
    let high: u8 = (0b1_000_000_1u8) | ((mux as u8) << 4);
    // LSB: DR=100 (128SPS), COMP disabled
    let low: u8 = 0b1000_0011;
    [ADS_REG_CONFIG, high, low]
}

/// Read single-ended channel from ADS1115, returns raw i16
fn ads_read_single(
    i2c: &mut I2cDriver,
    addr: u8,
    channel: u8,
) -> Result<i16, esp_idf_hal::i2c::I2cError> {
    let cfg = ads_config(channel);
    i2c.write(addr, &cfg, 50)?;
    FreeRtos::delay_ms(10); // wait for single-shot conversion (~8ms @ 128SPS)
    i2c.write(addr, &[ADS_REG_CONVERT], 50)?;
    let mut buf = [0u8; 2];
    i2c.read(addr, &mut buf, 50)?;
    Ok(i16::from_be_bytes(buf))
}

/// Convert raw ADC value to millivolts (GAIN_TWOTHIRDS = 6.144V FSR)
fn raw_to_mv(raw: i16) -> f32 {
    (raw as f32) * 6144.0 / 32767.0
}

// ─── Struktur penyimpanan data sensor ────────────────────
#[derive(Clone, Default)]
struct SensorData {
    // 8 kanal sensor gas
    mq6: f32,
    mq135: f32,
    mq3: f32,
    mq7: f32,
    tgs2611: f32,
    tgs2602: f32,
    tgs2600: f32,
    tgs2620: f32,
    // DHT22 environmental sensor
    dht_temp: f32,
    dht_hum: f32,
}

// ─── Timing helpers ───────────────────────────────────────
fn millis() -> u64 {
    EspSystemTime {}.now().as_millis() as u64
}

// ─── Baca 8 kanal sensor gas via ADS1115 ─────────────────
fn read_gas_sensors(i2c: &mut I2cDriver, data: &mut SensorData) {
    // ADS1115 pertama: MQ6, MQ135, MQ3, MQ7
    data.mq6   = raw_to_mv(ads_read_single(i2c, ADS1_ADDR, 0).unwrap_or(0));
    data.mq135 = raw_to_mv(ads_read_single(i2c, ADS1_ADDR, 1).unwrap_or(0));
    data.mq3   = raw_to_mv(ads_read_single(i2c, ADS1_ADDR, 2).unwrap_or(0));
    data.mq7   = raw_to_mv(ads_read_single(i2c, ADS1_ADDR, 3).unwrap_or(0));

    // ADS1115 kedua: TGS2611, TGS2602, TGS2600, TGS2620
    data.tgs2611 = raw_to_mv(ads_read_single(i2c, ADS2_ADDR, 0).unwrap_or(0));
    data.tgs2602 = raw_to_mv(ads_read_single(i2c, ADS2_ADDR, 1).unwrap_or(0));
    data.tgs2600 = raw_to_mv(ads_read_single(i2c, ADS2_ADDR, 2).unwrap_or(0));
    data.tgs2620 = raw_to_mv(ads_read_single(i2c, ADS2_ADDR, 3).unwrap_or(0));
}

// ─── Serialisasi data ke JSON (telemetry) ─────────────────
// Format: {"mq6":..., "mq135":..., ..., "dht_temp":..., "dht_hum":...}
fn build_telemetry_json(data: &SensorData) -> String {
    format!(
        concat!(
            "{{",
            "\"mq6\":{:.2},\"mq135\":{:.2},\"mq3\":{:.2},\"mq7\":{:.2},",
            "\"tgs2611\":{:.2},\"tgs2602\":{:.2},\"tgs2600\":{:.2},\"tgs2620\":{:.2},",
            "\"dht_temp\":{:.2},\"dht_hum\":{:.2}",
            "}}"
        ),
        data.mq6, data.mq135, data.mq3, data.mq7,
        data.tgs2611, data.tgs2602, data.tgs2600, data.tgs2620,
        data.dht_temp, data.dht_hum,
    )
}

// ─── Serialisasi device attributes ke JSON ────────────────
// Format: {"device":"E-Nose ESP32-S3","firmware":"1.0.0","sensor_count":8}
fn build_attributes_json() -> &'static str {
    "{\"device\":\"E-Nose ESP32-S3\",\"firmware\":\"1.0.0\",\"sensor_count\":8,\"adc\":\"ADS1115\"}"
}

// ─── Main ─────────────────────────────────────────────────
fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // ── I2C bus (SDA=GPIO8, SCL=GPIO9) @ 400kHz ──────────
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        pins.gpio8,  // SDA
        pins.gpio9,  // SCL
        &I2cConfig::new().baudrate(400_u32.kHz().into()),
    )?;

    // ── DHT22 sensor (GPIO10, open-drain with pull-up) ────
    let mut dht_pin = PinDriver::input_output_od(pins.gpio10, Pull::Up)?;

    println!(">>> Hardware initialized");

    // ── Koneksi Wi-Fi dengan retry (inline agar lifetime Modem tidak konflik) ──
    println!(">>> Menghubungkan ke Wi-Fi: {}", WIFI_SSID);
    let esp_wifi = EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs))?;
    let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop.clone())?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().unwrap(),
        password: WIFI_PASS.try_into().unwrap(),
        ..Default::default()
    }))?;
    wifi.start()?;
    println!(">>> Wi-Fi started, connecting...");

    // Mekanisme reconnect Wi-Fi: coba hingga 5x dengan jeda 3 detik
    let mut wifi_connected = false;
    for attempt in 1..=5 {
        println!(">>> Wi-Fi connect attempt {}/5...", attempt);
        match wifi.connect() {
            Ok(_) => {
                wifi.wait_netif_up()?;
                let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
                println!(">>> Wi-Fi terhubung! IP: {:?}", ip_info.ip);
                wifi_connected = true;
                break;
            }
            Err(e) => {
                println!(">>> Wi-Fi gagal (attempt {}): {:?}, retry dalam 3s...", attempt, e);
                FreeRtos::delay_ms(3000);
            }
        }
    }
    if !wifi_connected {
        println!(">>> Wi-Fi gagal setelah 5x percobaan. Reboot...");
        unsafe { esp_idf_svc::sys::esp_restart() };
    }

    // ── Konfigurasi MQTT client → ThingsBoard ─────────────
    let broker_url = format!("mqtt://{}:{}", TB_HOST.trim_start_matches("mqtt://"), TB_PORT);
    let mqtt_conf = MqttClientConfiguration {
        username: Some(TB_TOKEN), // ThingsBoard: username = access token
        ..Default::default()
    };

    println!(">>> Menghubungkan ke ThingsBoard MQTT: {}", broker_url);
    let (mut mqtt_client, mut mqtt_conn) =
        EspMqttClient::new(&broker_url, &mqtt_conf)?;

    // Spawn thread MQTT event loop dengan stack size eksplisit
    std::thread::Builder::new()
        .stack_size(16384)
        .spawn(move || loop {
            match mqtt_conn.next() {
                Ok(event) => {
                    println!("[MQTT] Event: {:?}", event.payload());
                }
                Err(e) => {
                    println!("[MQTT] Error: {:?}", e);
                    FreeRtos::delay_ms(1000);
                }
            }
        })
        .expect("Gagal spawn MQTT thread");

    // ── Kirim device attributes sekali saat boot ──────────
    let attr_json = build_attributes_json();
    if let Err(e) = mqtt_client.publish(
        TB_TOPIC_ATTRIBUTES,
        QoS::AtLeastOnce,
        false,
        attr_json.as_bytes(),
    ) {
        println!("[MQTT] Gagal publish attributes: {:?}", e);
    } else {
        println!(">>> Attributes terkirim: {}", attr_json);
    }

    println!(">>> Mulai loop pembacaan sensor (interval 5 detik)...");

    let mut last_read_ms: u64 = 0;
    let mut sensor_data = SensorData::default();
    let read_interval_ms: u64 = 5000; // baca dan kirim setiap 5 detik

    loop {
        let now = millis();

        // ── Baca sensor setiap interval ───────────────────
        if now.saturating_sub(last_read_ms) >= read_interval_ms {
            // Baca DHT22
            let mut delay = esp_idf_hal::delay::Ets;
            match dht22::Reading::read(&mut delay, &mut dht_pin) {
                Ok(reading) => {
                    sensor_data.dht_temp = reading.temperature;
                    sensor_data.dht_hum = reading.relative_humidity;
                }
                Err(_) => {
                    println!("[DHT22] Gagal membaca sensor");
                }
            }

            // Baca 8 kanal sensor gas via ADS1115
            read_gas_sensors(&mut i2c, &mut sensor_data);

            // Serialisasi ke JSON
            let telemetry_json = build_telemetry_json(&sensor_data);
            println!("[Sensor] {}", telemetry_json);

            // ── Publish telemetry ke ThingsBoard via MQTT ─
            match mqtt_client.publish(
                TB_TOPIC_TELEMETRY,
                QoS::AtMostOnce,
                false,
                telemetry_json.as_bytes(),
            ) {
                Ok(_) => println!("[MQTT] Telemetry terkirim OK"),
                Err(e) => println!("[MQTT] Gagal publish telemetry: {:?}", e),
            }

            last_read_ms = now;
        }

        FreeRtos::delay_ms(10);
    }
}