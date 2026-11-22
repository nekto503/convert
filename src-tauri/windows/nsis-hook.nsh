; NSIS-хук CONVERT: контекстное меню проводника + ассоциации файлов.
; Подключается автоматически tauri-action (nsis.hook в tauri.conf.json).
; Пишется в HKCU — не требует прав администратора и легко удаляется.

!macro CUSTOM_INSTALL
  ; --- Контекстное меню проводника: «Открыть в CONVERT» ---
  WriteRegStr HKCU "Software\Classes\*\shell\ConvertApp" "" "Открыть в CONVERT"
  WriteRegStr HKCU "Software\Classes\*\shell\ConvertApp" "Icon" "$INSTDIR\convert-desktop.exe"
  WriteRegStr HKCU "Software\Classes\*\shell\ConvertApp\command" "" '"$INSTDIR\convert-desktop.exe" "%1"'

  ; --- Папки: «Сжать в CONVERT» (открывает приложение с папкой) ---
  WriteRegStr HKCU "Software\Classes\Directory\shell\ConvertApp" "" "Открыть в CONVERT"
  WriteRegStr HKCU "Software\Classes\Directory\shell\ConvertApp" "Icon" "$INSTDIR\convert-desktop.exe"
  WriteRegStr HKCU "Software\Classes\Directory\shell\ConvertApp\command" "" '"$INSTDIR\convert-desktop.exe" "%1"'

  ; --- Ассоциации «Открыть с помощью» (без захвата расширений) ---
  ; CONVERT появляется в списке «Открыть с помощью» для常用 форматов,
  ; но не перехватывает их по умолчанию (уважение к выбору пользователя).
  !insertmacro REGISTER_OPENWITH "png"
  !insertmacro REGISTER_OPENWITH "jpg"
  !insertmacro REGISTER_OPENWITH "jpeg"
  !insertmacro REGISTER_OPENWITH "webp"
  !insertmacro REGISTER_OPENWITH "gif"
  !insertmacro REGISTER_OPENWITH "bmp"
  !insertmacro REGISTER_OPENWITH "svg"
  !insertmacro REGISTER_OPENWITH "pdf"
  !insertmacro REGISTER_OPENWITH "mp4"
  !insertmacro REGISTER_OPENWITH "mkv"
  !insertmacro REGISTER_OPENWITH "mp3"
  !insertmacro REGISTER_OPENWITH "wav"
  !insertmacro REGISTER_OPENWITH "flac"
  !insertmacro REGISTER_OPENWITH "docx"
  !insertmacro REGISTER_OPENWITH "xlsx"
  !insertmacro REGISTER_OPENWITH "pptx"
  !insertmacro REGISTER_OPENWITH "zip"
  !insertmacro REGISTER_OPENWITH "7z"
!macroend

!macro REGISTER_OPENWITH EXT
  WriteRegStr HKCU "Software\Classes\.${EXT}\OpenWithProgids" "ConvertApp.Document" ""
!macroend

!macro CUSTOM_UNINSTALL
  ; Полная очистка записей реестра.
  DeleteRegKey HKCU "Software\Classes\*\shell\ConvertApp"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\ConvertApp"
  !insertmacro UNREGISTER_OPENWITH "png"
  !insertmacro UNREGISTER_OPENWITH "jpg"
  !insertmacro UNREGISTER_OPENWITH "jpeg"
  !insertmacro UNREGISTER_OPENWITH "webp"
  !insertmacro UNREGISTER_OPENWITH "gif"
  !insertmacro UNREGISTER_OPENWITH "bmp"
  !insertmacro UNREGISTER_OPENWITH "svg"
  !insertmacro UNREGISTER_OPENWITH "pdf"
  !insertmacro UNREGISTER_OPENWITH "mp4"
  !insertmacro UNREGISTER_OPENWITH "mkv"
  !insertmacro UNREGISTER_OPENWITH "mp3"
  !insertmacro UNREGISTER_OPENWITH "wav"
  !insertmacro UNREGISTER_OPENWITH "flac"
  !insertmacro UNREGISTER_OPENWITH "docx"
  !insertmacro UNREGISTER_OPENWITH "xlsx"
  !insertmacro UNREGISTER_OPENWITH "pptx"
  !insertmacro UNREGISTER_OPENWITH "zip"
  !insertmacro UNREGISTER_OPENWITH "7z"
  DeleteRegValue HKCU "Software\Classes\ConvertApp.Document" ""
!macroend

!macro UNREGISTER_OPENWITH EXT
  DeleteRegValue HKCU "Software\Classes\.${EXT}\OpenWithProgids" "ConvertApp.Document"
!macroend
