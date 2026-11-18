# binkam — Yol Haritası

Her faz kendi başına çalışan bir sürümle biter. Bir faz bitmeden sonrakine geçilmez.
"Bitti" kriterleri sağlanmadan faz kapatılmaz.

---

## Faz 0 — İskelet ve güvenli terminal
Amaç: Boş ama sağlam bir döngü. Terminal asla bozuk kalmasın.

- [x] `cargo init --name binkam`, edition 2024, `git init`
- [x] Bağımlılıklar: `ratatui`, `color-eyre`, `clap` (derive)
- [x] `main.rs`: `color_eyre::install()`, `ratatui::init()`, `App::run`, `ratatui::restore()`
- [x] `app.rs`: sabit FPS döngüsü (varsayılan 30), `event::poll(timeout)` ile olay bekleme
- [x] Herhangi bir tuş → çıkış (yalnızca `KeyEventKind::Press`)
- [x] Açılışta ~300 ms "grace period": bu sürede gelen olaylar yok sayılır
      (programı başlatan Enter tuşunun bırakılması vb.)
- [x] `--dump-frame [--size WxH]`: `TestBackend` ile tek kare çiz, buffer'ı düz metin bas, çık
- [x] `Cargo.toml` release profili: `lto = true`, `codegen-units = 1`, `strip = true`

**Bitti:** Siyah ekran açılır, tuşa basınca kapanır; panic'te bile terminal düzgün döner;
`--dump-frame` çalışır; clippy temiz.

---

## Faz 1 — Büyük saat
Amaç: Ortada okunaklı, büyük bir dijital saat.

- [x] `clock/font.rs`: 0-9 ve `:` için blok karakter fontu (ör. 6x5, `█` ile)
- [x] `ClockWidget` (`Widget` trait'i): `HH:MM:SS`, `Layout` + `Flex::Center` ile ortalama
- [x] İki nokta saniyede bir yanıp söner
- [x] Altında Türkçe tarih: `15 Kasım 2026, Pazar` (gün/ay isimleri elle tanımlı dizilerden)
- [x] Boyut değişimine uyum; sığmazsa küçük `HH:MM` metnine düş
- [x] Testler: font genişliği/yüksekliği, Türkçe tarih biçimi, küçük alanda çökmeme
      (`TestBackend` ile)

**Bitti:** Saat ortada, her saniye güncelleniyor, pencere boyutlandırınca bozulmuyor.

---

## Faz 2 — Animasyon çatısı + ilk animasyon
Amaç: Genişletilebilir yapı ve saatin arkasında hareket.

- [x] `anim/mod.rs`: `Animation` trait'i (`resize` / `update` / `render`), `by_name`, `all_names`
- [x] `render.rs`: half-block piksel yardımcısı (`▀` ile hücre başına 2 dikey piksel),
      renk lerp fonksiyonu
- [x] `starfield.rs`: merkezden dışa akan 3D yıldız alanı, uzaklığa göre parlaklık
- [x] App: önce animasyon, üstüne saat çizilir; saat arkasında hafif koyu bir "kutu"
      (okunabilirlik için, ayarla kapatılabilir)
- [x] `--anim <isim>` CLI argümanı, `--list` ile isimleri listele
- [x] `--fps` göstergesi (debug için sağ üst köşe)
- [x] Test: sabit seed + sabit `dt` ile `update` deterministik

**Bitti:** Yıldız alanı akıcı, saat üstte okunaklı, CPU makul.

---

## Faz 3 — Animasyon kütüphanesi
Amaç: Çeşitlilik. Her animasyon ayrı commit, `add-animation` skill'i ile.

- [x] `matrix` — yeşil karakter yağmuru, kuyruk solması
- [x] `fire` — DOOM tarzı ateş (half-block, ısı paleti)
- [x] `plasma` — sinüs tabanlı truecolor plazma
- [x] `life` — Conway's Game of Life (half-block), durağanlaşınca yeniden tohumla
- [x] `snow` — süzülen kar taneleri, yatay salınım
- [x] `rain` — eğik yağmur çizgileri, yere çarpma sıçramaları
- [x] `waves` — üst üste binen sinüs dalgaları (half-block)
- [x] `pipes` — kalıcı iz bırakarak büyüyen borular, ekran dolunca temizlenir

**Bitti:** `--list` en az 5 animasyon gösteriyor, hepsi `--dump-frame` ile hatasız çiziliyor.

---

## Faz 4 — Kompozisyon, temalar, geçişler
Amaç: "Ekran koruyucu" hissi.

- [x] Saat konum modları: `center`, `bounce`, `corner`, `drift` (yavaş kayma — burn-in önleme)
      (`bounce` Faz 3'ten taşındı: saatin kendisi DVD logosu gibi sekip her çarpışmada
      renk değiştirir — bu bir arka plan animasyonu değil, saat konum modu)
- [x] Otomatik döngü: her N dakikada bir sonraki/rastgele animasyon
- [x] Geçiş efekti: eski ve yeni animasyon arasında kısa karartma (fade to black → fade in)
- [x] Renk temaları: `default`, `nord`, `gruvbox`, `amber`, `mono` — saat ve animasyonlar
      temadan renk alır
- [x] Önizleme modu (`--preview`): tuşlar çıkmaz; `n`/`p` sonraki/önceki animasyon,
      `t` tema, `q`/`Esc` çıkış

**Bitti:** Uzun süre açık bırakıldığında kendi kendine çeşitleniyor; önizleme modunda tüm
kombinasyonlar gezilebiliyor.

---

## Faz 5 — Ayarlar ve CLI
Amaç: Kullanıcı kodu değiştirmeden özelleştirebilsin.

- [x] `config.rs`: `serde` + `toml`; yol `directories::ProjectDirs` ile
      (Linux `~/.config/binkam/config.toml`, Windows `%APPDATA%\binkam\...`)
- [x] Öncelik: CLI > config dosyası > varsayılan
- [x] Seçenekler: `fps`, `anim` (veya liste), `cycle_minutes`, `theme`, `clock_position`,
      `show_seconds`, `show_date`, `hour_format` (24/12), `clock_backdrop`, `seed`
- [x] `--print-config`: geçerli birleşik ayarı TOML olarak bas
- [x] Bozuk config → anlaşılır hata mesajı (satır numarasıyla), çökme değil
- [x] Test: birleştirme önceliği, varsayılanlar, hatalı TOML

**Bitti:** Örnek `config.toml` README'de; her seçenek CLI'dan ve dosyadan çalışıyor.

---

## Faz 6 — Cila ve performans
Amaç: Her terminalde düzgün, hafif.

- [x] Renk desteği tespiti: `COLORTERM` yoksa 256 renge düş; `--no-color` modu
- [x] Fare hareketi/tıklama ile çıkış (mouse capture aç, çıkışta mutlaka kapat)
- [x] CPU ölçümü (release, 30 FPS, 200x50): her animasyon için not et; ağır olanı optimize et
      (render'da allocation yok, gereksiz `Color::Rgb` hesapları önbellekte)
- [x] Çok küçük / çok büyük terminal, hızlı yeniden boyutlandırma testleri
- [ ] Windows Terminal + PowerShell'de elle test (kullanıcı)
- [x] `clippy::pedantic` uyarılarını gözden geçir (hepsini değil, anlamlı olanları düzelt)
- [x] README: kurulum, kullanım, ekran görüntüsü/GIF (ör. `vhs` ile)

**Bitti:** İki işletim sisteminde test edildi, CPU tablosu README'de.

---

## Faz 7 — Gerçek ekran koruyucu entegrasyonu ve dağıtım
Amaç: Gerçekten "boşta kalınca" çalışsın.

- [x] `cargo install --path .` ile kurulum talimatı
- [x] tmux entegrasyonu: `set -g lock-after-time 300` + `set -g lock-command "binkam"`
- [x] (opsiyonel) Linux masaüstünde boşta kalma aracından (ör. hypridle / swayidle)
      tam ekran terminalde başlatma örneği
- [x] GitHub Actions: Linux + Windows'ta `fmt`, `clippy`, `test`, release build
- [x] Sürüm etiketi `v0.1.0`, release binary'leri

**Bitti:** tmux'ta 5 dk boşta kalınca binkam açılıyor; CI yeşil.

---

## Faz 8 — Uygulama içinden ayar yapma
Amaç: `binkam`'ı açınca her şeyi oradan ayarlayabilmek, ayarların kalıcı olması.

- [x] Saat fontu: birden fazla blok font (`block`, `shadow`, `slim`, `tall`)
- [x] Saat boyutu: `clock_scale` 1-4, her glyph pikseli kare bir hücre bloğuna
      büyüsün; sığmazsa kademe kademe küçülsün
- [x] `Config::save`: ayarları TOML olarak geri yaz (seed hariç)
- [x] Altta nvim tarzı tuş çubuğu; ekrana sığan en geniş sürüm çizilsin
- [x] Her ayar için sol üstte açılan liste: oklarla gezinirken ekran anında
      değişsin, Enter kaydetsin, Esc geri alsın
- [x] `?` ile tüm tuşların listesi
- [x] `--lock`: eski ekran koruyucu davranışı (her tuş çıkar), kilit/boşta
      komutları bunu kullansın
- [x] README, demo.tape ve tmux satırı güncellensin

**Bitti:** `binkam` ile açıp tema/font/boyut değiştir, çık, tekrar aç — aynı
şekilde açılıyor.

---

## Fikir havuzu (sonra)
- Analog saat (Canvas + Braille)
- Pomodoro / geri sayım modu
- Hava durumu satırı
- Ses seviyesine tepki veren animasyon
- Kendi animasyonunu Lua/Rhai script ile tanımlama
