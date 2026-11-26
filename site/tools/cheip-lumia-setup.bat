@echo off
rem Avvia lo script come amministratore
powershell -NoProfile -Command "Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File \"%~dp0cheip-lumia-setup.ps1\" -CheipDefault'"
