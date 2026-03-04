---
name: weather
description: Get weather forecast from Open-Meteo service (free, no API key required)
always: false
bins:
  - curl
  - jq
---

# Weather Forecast Skill

Get weather forecasts from Open-Meteo - a free, open-source weather API with no API key required.

## Basic Usage

### Current Weather

```bash
# Current weather for specific coordinates (latitude, longitude)
curl -s "https://api.open-meteo.com/v1/forecast?latitude=23.1291&longitude=113.2644&current=temperature_2m,weather_code,relative_humidity_2m,wind_speed_10m&timezone=auto"

# Get JSON and parse with jq
curl -s "https://api.open-meteo.com/v1/forecast?latitude=23.1291&longitude=113.2644&current=temperature_2m,weather_code,relative_humidity_2m,wind_speed_10m&timezone=auto" | jq '.current'
```

### Multi-day Forecast

```bash
# 3-day forecast with daily max/min temperatures
curl -s "https://api.open-meteo.com/v1/forecast?latitude=23.1291&longitude=113.2644&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max&timezone=auto&forecast_days=3" | jq '.daily'
```

## Location Examples

| City | Latitude | Longitude |
|------|----------|-----------|
| Guangzhou | 23.1291 | 113.2644 |
| Beijing | 39.9042 | 116.4074 |
| Shanghai | 31.2304 | 121.4737 |
| Shenzhen | 22.5431 | 114.0579 |
| Chengdu | 30.5728 | 104.0668 |

```bash
# Beijing weather
curl -s "https://api.open-meteo.com/v1/forecast?latitude=39.9042&longitude=116.4074&current=temperature_2m,weather_code&timezone=auto"

# Shanghai weather
curl -s "https://api.open-meteo.com/v1/forecast?latitude=31.2304&longitude=121.4737&current=temperature_2m,weather_code&timezone=auto"
```

## Available Parameters

### Current Weather Parameters

```bash
# Temperature (°C)
current=temperature_2m

# Weather code (WMO code)
current=weather_code

# Relative humidity (%)
current=relative_humidity_2m

# Wind speed (km/h)
current=wind_speed_10m

# Wind direction (°)
current=wind_direction_10m

# Apparent temperature (feels like, °C)
current=apparent_temperature

# Precipitation (mm)
current=precipitation

# Cloud cover (%)
current=cloud_cover

# Pressure (hPa)
current=surface_pressure
```

### Daily Forecast Parameters

```bash
# Max/min temperature
daily=temperature_2m_max,temperature_2m_min

# Weather code
daily=weather_code

# Precipitation probability (%)
daily=precipitation_probability_max

# Precipitation sum (mm)
daily=precipitation_sum

# Wind speed max (km/h)
daily=wind_speed_10m_max

# Sunrise/sunset
daily=sunrise,sunset

# UV index max
daily=uv_index_max
```

## Query Options

```bash
# Number of forecast days (1-16)
&forecast_days=3

# Timezone (auto or specific)
&timezone=auto
&timezone=Asia/Shanghai

# Units (metric or imperial)
&temperature_unit=celsius
&temperature_unit=fahrenheit
&windspeed_unit=kmh
&windspeed_unit=ms
```

## Weather Codes (WMO)

| Code | Description |
|------|-------------|
| 0 | Clear sky |
| 1, 2, 3 | Mainly clear, partly cloudy, overcast |
| 45, 48 | Fog, depositing rime fog |
| 51, 53, 55 | Drizzle: Light, moderate, dense |
| 61, 63, 65 | Rain: Slight, moderate, heavy |
| 71, 73, 75 | Snow fall: Slight, moderate, heavy |
| 80, 81, 82 | Rain showers: Slight, moderate, violent |
| 95, 96, 99 | Thunderstorm: Slight/moderate, heavy, with hail |

## Practical Examples

### Get Current Weather Summary (Guangzhou)

```bash
curl -s "https://api.open-meteo.com/v1/forecast?latitude=23.1291&longitude=113.2644&current=temperature_2m,weather_code,relative_humidity_2m,wind_speed_10m,apparent_temperature&timezone=auto" | jq '{
  temperature: .current.temperature_2m,
  feels_like: .current.apparent_temperature,
  weather_code: .current.weather_code,
  humidity: .current.relative_humidity_2m,
  wind_speed: .current.wind_speed_10m,
  time: .current.time
}'
```

### Get 3-Day Forecast

```bash
curl -s "https://api.open-meteo.com/v1/forecast?latitude=23.1291&longitude=113.2644&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset&timezone=auto&forecast_days=3" | jq '{
  location: {lat: 23.1291, lon: 113.2644},
  forecast: [.daily.time, .daily.temperature_2m_max, .daily.temperature_2m_min, .daily.weather_code] | transpose | map({
    date: .[0],
    max_temp: .[1],
    min_temp: .[2],
    weather_code: .[3]
  })
}'
```

### Get Hourly Forecast for Today

```bash
curl -s "https://api.open-meteo.com/v1/forecast?latitude=23.1291&longitude=113.2644&hourly=temperature_2m,weather_code,precipitation_probability&timezone=auto&forecast_days=1" | jq '{
  hourly: [.hourly.time, .hourly.temperature_2m, .hourly.weather_code] | transpose | map({
    time: .[0],
    temp: .[1],
    weather_code: .[2]
  }) | .[0:6]  # Next 6 hours
}'
```

### Complete Weather Report Script

```bash
#!/bin/bash
# Weather report for Guangzhou

LAT=23.1291
LON=113.2644
CITY="Guangzhou"

curl -s "https://api.open-meteo.com/v1/forecast?latitude=$LAT&longitude=$LON&current=temperature_2m,weather_code,relative_humidity_2m,wind_speed_10m,apparent_temperature&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset&timezone=auto&forecast_days=3" | jq --arg city "$CITY" '{
  location: $city,
  coordinates: {latitude: $LAT, longitude: $LON},
  current: {
    temperature: .current.temperature_2m,
    feels_like: .current.apparent_temperature,
    weather_code: .current.weather_code,
    humidity: .current.relative_humidity_2m,
    wind_speed: .current.wind_speed_10m,
    time: .current.time
  },
  forecast: [.daily.time, .daily.temperature_2m_max, .daily.temperature_2m_min, .daily.weather_code] | transpose | map({
    date: .[0],
    max_temp: .[1],
    min_temp: .[2],
    weather_code: .[3]
  })
}'
```

## Advanced Features

### Air Quality Data (Separate Endpoint)

```bash
# PM2.5, PM10, CO, NO2, SO2, O3
curl -s "https://air-quality-api.open-meteo.com/v1/air-quality?latitude=23.1291&longitude=113.2644&current=pm2_5,pm10,carbon_monoxide,nitrogen_dioxide,sulphur_dioxide,ozone&timezone=auto"
```

### Historical Weather Data

```bash
# Past weather (start_date, end_date)
curl -s "https://api.open-meteo.com/v1/forecast?latitude=23.1291&longitude=113.2644&daily=temperature_2m_max,temperature_2m_min&start_date=2024-01-01&end_date=2024-01-07&timezone=auto"
```

### Marine Weather (Sea Temperature, Waves)

```bash
# Sea surface temperature and wave data
curl -s "https://marine-api.open-meteo.com/v1/marine?latitude=23.13&longitude=113.26&current=sea_surface_temperature,wave_height&timezone=auto"
```

## Tips

1. **No API Key Required**: Open-Meteo is completely free for non-commercial use
2. **Rate Limits**: 10,000 API calls per day (generous for personal use)
3. **Auto Timezone**: Use `&timezone=auto` for automatic timezone detection
4. **Combine Parameters**: Separate multiple parameters with commas
5. **JSON Output**: Always returns JSON, easy to parse with `jq`

## Comparison with wttr.in

| Feature | Open-Meteo | wttr.in |
|---------|------------|---------|
| API Key | ❌ Not required | ❌ Not required |
| Format | JSON only | Text/ASCII/JSON |
| Rate Limit | 10,000/day | Undocumented |
| Reliability | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ |
| Data Sources | Multiple (ECMWF, GFS, etc.) | Various |
| Historical Data | ✅ Yes | ❌ Limited |
| Air Quality | ✅ Separate API | ❌ No |
| Marine Weather | ✅ Separate API | ❌ No |

## Documentation

- Main API: https://open-meteo.com/en/docs
- Air Quality API: https://open-meteo.com/en/docs/air-quality-api
- Marine API: https://open-meteo.com/en/docs/marine-api
- GitHub: https://github.com/open-meteo/open-meteo
