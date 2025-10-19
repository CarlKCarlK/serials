# CYW43 Firmware Files

Download the firmware files from the official Raspberry Pi repository:

```powershell
# Download firmware blob
Invoke-WebRequest -Uri "https://raw.githubusercontent.com/georgerobotics/cyw43-driver/main/firmware/43439A0.bin" -OutFile "43439A0.bin"

# Download CLM (Country Locale Matrix) blob  
Invoke-WebRequest -Uri "https://raw.githubusercontent.com/georgerobotics/cyw43-driver/main/firmware/43439A0_clm.bin" -OutFile "43439A0_clm.bin"
```

Or use curl:
```powershell
curl -L -o 43439A0.bin https://raw.githubusercontent.com/georgerobotics/cyw43-driver/main/firmware/43439A0.bin
curl -L -o 43439A0_clm.bin https://raw.githubusercontent.com/georgerobotics/cyw43-driver/main/firmware/43439A0_clm.bin
```

These files are required for the Pico W WiFi to function.
