# Keycast Release Build & Packaging Automation Script
$ErrorActionPreference = "Stop"

$ProjectRoot = "C:\Users\Public\Music\project"
$CertPath = Join-Path $ProjectRoot "certs\Keycast.pfx"
$CertCer = Join-Path $ProjectRoot "certs\Keycast.cer"
$CertPassword = "Keycast2026!"
$SignTool = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe"
$Iscc = "C:\Users\sai\AppData\Local\Programs\Inno Setup 6\ISCC.exe"

Write-Host "=============================================" -ForegroundColor Cyan
Write-Host "   Keycast Release Build & Sign Pipeline" -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan

# 1. Check or generate icon
if (-not (Test-Path "$ProjectRoot\assets\keycast.ico")) {
    Write-Host "[1/6] Generating application icon..." -ForegroundColor Yellow
    & powershell -ExecutionPolicy Bypass -File "$ProjectRoot\generate_icon.ps1"
} else {
    Write-Host "[1/6] Icon exists: assets\keycast.ico" -ForegroundColor Green
}

# 2. Check or generate Code Signing Certificate
if (-not (Test-Path $CertPath) -or -not (Test-Path $CertCer)) {
    Write-Host "[2/6] Generating code signing certificate..." -ForegroundColor Yellow
    New-Item -ItemType Directory -Force -Path "$ProjectRoot\certs" | Out-Null
    $cert = New-SelfSignedCertificate -Type CodeSigningCert `
        -Subject "CN=Keycast Project, O=Keycast, OU=Keycast Development, C=US" `
        -KeyUsage DigitalSignature `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -NotAfter (Get-Date).AddYears(10) `
        -FriendlyName "Keycast Open Source Project Code Signing"

    $pwd = ConvertTo-SecureString -String $CertPassword -Force -AsPlainText
    Export-PfxCertificate -Cert $cert -FilePath $CertPath -Password $pwd | Out-Null
    Export-Certificate -Cert $cert -FilePath $CertCer | Out-Null

    $rootStore = New-Object System.Security.Cryptography.X509Certificates.X509Store("Root", "CurrentUser")
    $rootStore.Open("ReadWrite")
    $rootStore.Add($cert)
    $rootStore.Close()

    $pubStore = New-Object System.Security.Cryptography.X509Certificates.X509Store("TrustedPublisher", "CurrentUser")
    $pubStore.Open("ReadWrite")
    $pubStore.Add($cert)
    $pubStore.Close()
} else {
    Write-Host "[2/6] Code signing certificate ready: certs\Keycast.pfx" -ForegroundColor Green
}

# 3. Build Keycast in release mode
Write-Host "[3/6] Building Keycast binary (cargo build --release)..." -ForegroundColor Yellow
Push-Location $ProjectRoot
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

# 4. Sign keycast.exe
Write-Host "[4/6] Signing target\release\keycast.exe..." -ForegroundColor Yellow
& $SignTool sign /f $CertPath /p $CertPassword /fd SHA256 /d "Keycast" /du "https://github.com" "$ProjectRoot\target\release\keycast.exe"
if ($LASTEXITCODE -ne 0) { throw "signtool failed on keycast.exe" }

# Copy updated binary & source to dist\Keycast
Copy-Item "$ProjectRoot\target\release\keycast.exe" "$ProjectRoot\dist\Keycast\keycast.exe" -Force
Copy-Item "$ProjectRoot\certs\Keycast.cer" "$ProjectRoot\dist\Keycast\Keycast.cer" -Force
Copy-Item "$ProjectRoot\README.md" "$ProjectRoot\dist\Keycast\README.md" -Force

# Update dist\Keycast\source
Copy-Item "$ProjectRoot\Cargo.toml" "$ProjectRoot\dist\Keycast\source\Cargo.toml" -Force
Copy-Item "$ProjectRoot\Cargo.lock" "$ProjectRoot\dist\Keycast\source\Cargo.lock" -Force
Copy-Item "$ProjectRoot\build.rs" "$ProjectRoot\dist\Keycast\source\build.rs" -Force
Copy-Item "$ProjectRoot\app.manifest" "$ProjectRoot\dist\Keycast\source\app.manifest" -Force
Copy-Item "$ProjectRoot\app.rc" "$ProjectRoot\dist\Keycast\source\app.rc" -Force
New-Item -ItemType Directory -Force -Path "$ProjectRoot\dist\Keycast\source\assets" | Out-Null
Copy-Item "$ProjectRoot\assets\keycast.ico" "$ProjectRoot\dist\Keycast\source\assets\keycast.ico" -Force
Copy-Item "$ProjectRoot\src\*" "$ProjectRoot\dist\Keycast\source\src\" -Recurse -Force

# 5. Build Installer with Inno Setup
Write-Host "[5/6] Building installer executable with Inno Setup (ISCC)..." -ForegroundColor Yellow
& $Iscc "$ProjectRoot\installer.iss"
if ($LASTEXITCODE -ne 0) { throw "ISCC compilation failed" }

# Sign the installer executable
Write-Host "Signing dist\Keycast-Setup.exe..." -ForegroundColor Yellow
& $SignTool sign /f $CertPath /p $CertPassword /fd SHA256 /d "Keycast Installer" /du "https://github.com" "$ProjectRoot\dist\Keycast-Setup.exe"
if ($LASTEXITCODE -ne 0) { throw "signtool failed on Keycast-Setup.exe" }

# Copy Keycast-Setup.exe to root for quick access
Copy-Item "$ProjectRoot\dist\Keycast-Setup.exe" "$ProjectRoot\Keycast-Setup.exe" -Force
Copy-Item "$ProjectRoot\certs\Keycast.cer" "$ProjectRoot\Keycast.cer" -Force

# 6. Create helper batch files and package ZIP
Write-Host "[6/6] Generating portable helper scripts and ZIP package..." -ForegroundColor Yellow

# Helper batch script for Setup on other PCs
$installBat = @"
@echo off
setlocal
title Keycast Installer
cd /d "%~dp0"
echo ========================================================
echo   Keycast - Setup and Certificate Trust Helper
echo ========================================================
echo.
echo [1/3] Adding certificate to Current User Trusted Root store...
certutil.exe -user -addstore Root Keycast.cer >nul 2>&1
echo [2/3] Adding certificate to Current User Trusted Publisher store...
certutil.exe -user -addstore TrustedPublisher Keycast.cer >nul 2>&1
certutil.exe -user -addstore TrustedPeople Keycast.cer >nul 2>&1
echo [3/3] Removing Mark-of-the-Web (Zone.Identifier) if present...
powershell -NoProfile -ExecutionPolicy Bypass -Command "Get-ChildItem -LiteralPath '%~dp0' | Unblock-File" >nul 2>&1
echo.
echo Starting Keycast Setup...
start "" "%~dp0Keycast-Setup.exe"
"@
Set-Content -Path "$ProjectRoot\dist\Install-Keycast.bat" -Value $installBat -Encoding ASCII
Set-Content -Path "$ProjectRoot\Install-Keycast.bat" -Value $installBat -Encoding ASCII

# Helper batch script for portable run
$runBat = @"
@echo off
setlocal
title Keycast
cd /d "%~dp0"
certutil.exe -user -addstore Root "%~dp0Keycast.cer" >nul 2>&1
certutil.exe -user -addstore TrustedPublisher "%~dp0Keycast.cer" >nul 2>&1
certutil.exe -user -addstore TrustedPeople "%~dp0Keycast.cer" >nul 2>&1
powershell -NoProfile -ExecutionPolicy Bypass -Command "Get-ChildItem -LiteralPath '%~dp0' -Recurse | Unblock-File" >nul 2>&1
start "" "%~dp0keycast.exe"
"@
Set-Content -Path "$ProjectRoot\dist\Keycast\Run-Keycast.bat" -Value $runBat -Encoding ASCII

$trustBat = @"
@echo off
setlocal
title Trust Keycast Certificate
cd /d "%~dp0"
echo Registering Keycast digital certificate in Current User Trusted Store...
certutil.exe -user -addstore Root "%~dp0Keycast.cer"
certutil.exe -user -addstore TrustedPublisher "%~dp0Keycast.cer"
certutil.exe -user -addstore TrustedPeople "%~dp0Keycast.cer"
powershell -NoProfile -ExecutionPolicy Bypass -Command "Get-ChildItem -LiteralPath '%~dp0' -Recurse | Unblock-File"
echo.
echo Certificate trusted and files unblocked successfully!
pause
"@
Set-Content -Path "$ProjectRoot\dist\Keycast\Trust-Certificate.bat" -Value $trustBat -Encoding ASCII

# Re-create Keycast-Windows.zip
Push-Location "$ProjectRoot\dist"
try {
    if (Test-Path "$ProjectRoot\Keycast-Windows.zip") {
        Remove-Item "$ProjectRoot\Keycast-Windows.zip" -Force
    }
    tar -a -cf "$ProjectRoot\Keycast-Windows.zip" Keycast
} finally {
    Pop-Location
}

Write-Host "=============================================" -ForegroundColor Green
Write-Host "   Build & Packaging Completed Successfully!" -ForegroundColor Green
Write-Host "=============================================" -ForegroundColor Green
Write-Host "Installer: $ProjectRoot\Keycast-Setup.exe" -ForegroundColor Cyan
Write-Host "Portable ZIP: $ProjectRoot\Keycast-Windows.zip" -ForegroundColor Cyan
