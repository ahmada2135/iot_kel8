# Smart Coffee Processing: IoT-Based Electronic Nose ☕👃

**Kelompok 8**
- Khusnul Khuluq Ahmada (2042241001)
- Galen Zahid Wajendra (2042241044)

Repositori ini berisi *firmware* untuk sistem **Electronic Nose (E-Nose)** berbasis ESP32-S3 yang ditulis menggunakan **embedded Rust**. Proyek ini merupakan bagian dari *Case-Based Project* Mata Kuliah Teknologi IoT.

Sistem ini bertugas untuk membaca data dari deretan sensor gas (MOS) dan sensor lingkungan, lalu mengirimkan data tersebut ke ThingsBoard Cloud melalui protokol MQTT untuk keperluan klasifikasi profil aroma kopi (sebelum diolah lebih lanjut dengan TinyML).

## 🎯 Progres & Checklist Tugas (Kelas A)

Berikut adalah daftar target fitur yang telah berhasil diimplementasikan pada *firmware* ini:

- [x] **Firmware ESP32-S3 berbasis Rust**
- [x] **Komunikasi I2C dengan ADS1115**
- [x] **Pemetaan delapan kanal sensor gas** (MQ6, MQ135, MQ3, MQ7, TGS2611, TGS2602, TGS2600, TGS2620)
- [x] **Pembacaan sensor DHT22** (suhu dan kelembapan)
- [x] **Struktur penyimpanan data sensor**
- [x] **Serialisasi data menggunakan JSON**
- [x] **Konfigurasi konektivitas Wi-Fi**
- [x] **Mekanisme reconnect Wi-Fi** (maksimal 5x percobaan sebelum sistem otomatis *reboot*)
- [x] **MQTT client**
- [x] **Konfigurasi ThingsBoard Cloud**
- [x] **Pembuatan device ThingsBoard**
- [x] **Autentikasi menggunakan access token**
- [x] **Topic telemetry** (`v1/devices/me/telemetry`)
- [x] **Topic attributes** (`v1/devices/me/attributes`)
- [x] **Mekanisme reconnect MQTT** (menggunakan *event loop* pada *thread* terpisah berkapasitas 16KB)

## 🛠️ Cara Menjalankan (Build & Flash)

Pastikan kamu sudah menginstal *toolchain* Rust untuk ESP-IDF.

1. Sesuaikan kredensial di dalam `src/main.rs`:
   ```rust
   const WIFI_SSID: &str = "SSID_KAMU";
   const WIFI_PASS: &str = "PASSWORD_KAMU";
   const TB_TOKEN: &str = "TOKEN_THINGSBOARD_KAMU";
   ```
2. Hubungkan board ESP32-S3 ke komputer.
3. Jalankan perintah berikut untuk *build*, *flash*, dan memantau *serial monitor*:
   ```sh
   cargo run
   ```

## ☁️ Dashboard & Visualisasi (Kelas C)
Data dari perangkat ini akan dikirimkan setiap 5 detik ke cloud (`eu.thingsboard.cloud`). Teman-teman dari Kelas C dapat menggunakan *Access Token* yang sama untuk membuat *Dashboard* visualisasi data sensor secara real-time.
