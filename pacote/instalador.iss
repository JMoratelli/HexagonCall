; Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
; Licenca GNU GPL 3 ou posterior.
;
; instalador.iss - o instalador do Hexagon Call para Windows (Inno Setup 6),
; no molde do Atalaia Acessos (JMoratelli/acessos, scripts/instalador.iss).
;
; Nao compila nada: empacota o HexagonCall.exe que pacote/windows.ps1 ja
; gerou na pasta de saida. Rodar por ali:
;
;     pwsh pacote/windows.ps1
;
; A versao chega de fora (/DAppVersion=X.Y.Z), lida do Cargo.toml da raiz: a
; mesma do executavel, para o atualizador (janela/src/atualizador.rs) nunca
; ver um instalador com versao diferente do app que ele instala.
#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#ifndef Saida
  #define Saida "saida"
#endif

[Setup]
; AppId FIXO: e o que faz o Inno reconhecer "isto ja esta instalado, e uma
; atualizacao" em vez de deixar duas copias. NUNCA gerar outro depois da
; primeira distribuicao. O "{{" escapa a chave literal.
AppId={{7f9ce912-2d94-416a-9f93-a43b80c3b731}
AppName=Hexagon Call
AppVersion={#AppVersion}
AppVerName=Hexagon Call {#AppVersion}
AppPublisher=Jurandir Moratelli
AppPublisherURL=https://github.com/JMoratelli/HexagonCall
; Sem administrador: instala no perfil do usuario (%LOCALAPPDATA%\Programs),
; e a atualizacao, que roda este mesmo instalador, tambem nao pede nada.
PrivilegesRequired=lowest
; Pasta SEM espaco: o "abrir com o sistema" (tauri-plugin-autostart) grava o
; caminho do exe no Run do registro sem aspas.
DefaultDirName={autopf}\HexagonCall
DefaultGroupName=Hexagon Call
DisableProgramGroupPage=yes
DisableDirPage=auto
OutputBaseFilename=HexagonCallSetup-{#AppVersion}
SetupIconFile=..\janela\icons\icon.ico
UninstallDisplayIcon={app}\HexagonCall.exe
UninstallDisplayName=Hexagon Call
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupLogging=yes
; Na atualizacao quem fecha o app e o proprio app, logo depois de disparar
; este instalador: ele desregistra as contas no PBX e tira as notificacoes
; antes de sair (janela/src/atualizador.rs). O Restart Manager fica de rede de
; seguranca, e "force" porque um processo que nao sai sozinho nao pode travar a
; atualizacao de quem nem esta olhando a tela.
CloseApplications=force
; Reabrir e o [Run] la embaixo, que cobre tambem o app que ja tinha saido.
RestartApplications=no

[Languages]
Name: "brportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Files]
Source: "{#Saida}\HexagonCall.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
; A tela de espera da ATUALIZACAO (ver [Code]): imagem e fontes vao so para o
; {tmp}, nao sao instaladas. O icone sai do icon.png do app (gerado pelo
; windows.ps1); as fontes sao as que a tela do app usa.
Source: "{#Saida}\espera-icone.bmp"; Flags: dontcopy
Source: "..\janela\ui\fontes\Sora-SemiBold.ttf"; Flags: dontcopy
Source: "..\janela\ui\fontes\SpaceGrotesk-Regular.ttf"; Flags: dontcopy

[Icons]
; O AppUserModelID no atalho e o que da nome e icone ao toast da ligacao (sem
; ele, o Windows mostra "Windows PowerShell"). O mesmo de janela/src/aumid.rs.
Name: "{autoprograms}\Hexagon Call"; Filename: "{app}\HexagonCall.exe"; WorkingDir: "{app}"; AppUserModelID: "org.jj.HexagonCall"
Name: "{autodesktop}\Hexagon Call"; Filename: "{app}\HexagonCall.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Criar um atalho na Área de Trabalho"; GroupDescription: "Atalhos adicionais:"

[Registry]
; O nome do app para as notificacoes. O app grava de novo ao abrir (com o
; icone, e o do flavor M Calls); aqui fica para o desinstalador apagar.
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\org.jj.HexagonCall"; ValueType: string; ValueName: "DisplayName"; ValueData: "Hexagon Call"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\org.jj.HexagonCall.MCalls"; ValueType: none; Flags: uninsdeletekey dontcreatekey
; "Abrir com o sistema" e do app (Ajustes, Janela); desinstalado, sai daqui.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Hexagon Call"; Flags: uninsdeletevalue dontcreatekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"; ValueType: none; ValueName: "Hexagon Call"; Flags: uninsdeletevalue dontcreatekey

[Run]
; skipifsilent: a instalacao comum, com a caixa "abrir agora" no fim.
Filename: "{app}\HexagonCall.exe"; Description: "Abrir o Hexagon Call agora"; Flags: nowait postinstall skipifsilent
; skipifnotsilent: o complemento, para o atualizador (que SEMPRE roda
; /VERYSILENT): reabre sem perguntar.
Filename: "{app}\HexagonCall.exe"; Flags: nowait skipifnotsilent

[UninstallDelete]
; Os icones que o app grava para as notificacoes, e a pasta so se ficar vazia:
; as gravacoes das ligacoes moram nela (gravacoes\). Contas, historico e
; conversas (%APPDATA%\hexagon-call) tambem ficam: sao do usuario, e
; reinstalar os encontra.
Type: files; Name: "{localappdata}\hexagon-call\*.png"
Type: dirifempty; Name: "{localappdata}\hexagon-call"

[Code]
// ---------------------------------------------------------------------
// A TELA DE ESPERA DA ATUALIZACAO (a mesma ideia do Acessos)
// ---------------------------------------------------------------------
//
// So aparece na instalacao SILENCIOSA, que e como o atualizador do app roda
// este instalador. Na atualizacao o app sai e so volta no [Run], depois da
// copia: sem esta tela, quem esta olhando ve o app sumir e conclui que travou.
// O instalador e o unico processo vivo do comeco ao fim, entao e ele quem
// mostra. A cara e a do app: cartao claro, o verde do Hexagon Call, o icone e
// as fontes da tela (carregadas so para este processo).

const
  // Paleta do tema claro (janela/ui/estilo.css). TColor e BGR:
  //   Cartao #ffffff -> $FFFFFF    Borda #dcddd3 -> $D3DDDC
  //   Texto  #1c1f1b -> $1B1F1C    Sec   #55594f -> $4F5955
  //   Verde  #087f71 -> $717F08    Trilho #ebece4 -> $E4ECEB
  corCartao = $FFFFFF;
  corBorda  = $D3DDDC;
  corTexto  = $1B1F1C;
  corSec    = $4F5955;
  corVerde  = $717F08;
  corTrilho = $E4ECEB;
  FR_PRIVATE = $10;
  HWND_TOPMOST = -1;
  SWP_NOSIZE = $1;
  SWP_NOMOVE = $2;
  SWP_NOACTIVATE = $10;
  RDW_INVALIDATE = $1;
  RDW_ALLCHILDREN = $80;
  RDW_UPDATENOW = $100;

function AddFontResourceEx(lpszFilename: string; fl: DWORD; pdv: Integer): Integer;
  external 'AddFontResourceExW@gdi32.dll stdcall';
function SetWindowPos(hWnd: HWND; hWndInsertAfter: Integer; X, Y, cx, cy: Integer;
  uFlags: UINT): BOOL; external 'SetWindowPos@user32.dll stdcall';
function RedrawWindow(hWnd: HWND; lprcUpdate: Integer; hrgnUpdate: Integer;
  flags: UINT): BOOL; external 'RedrawWindow@user32.dll stdcall';

var
  formEspera: TSetupForm;
  trilho, barra: TPanel;
  instalado: Boolean;

procedure AbrirEspera;
var
  moldura: TPanel;
  img: TBitmapImage;
  titulo, aviso: TNewStaticText;
begin
  ExtractTemporaryFile('espera-icone.bmp');
  ExtractTemporaryFile('Sora-SemiBold.ttf');
  ExtractTemporaryFile('SpaceGrotesk-Regular.ttf');
  // Privadas: valem so para este processo.
  AddFontResourceEx(ExpandConstant('{tmp}\Sora-SemiBold.ttf'), FR_PRIVATE, 0);
  AddFontResourceEx(ExpandConstant('{tmp}\SpaceGrotesk-Regular.ttf'), FR_PRIVATE, 0);

  formEspera := CreateCustomForm(ScaleX(440), ScaleY(232), False, True);
  formEspera.BorderStyle := bsNone;
  formEspera.Position := poScreenCenter;
  formEspera.Color := corBorda;

  // Moldura: 1px da cor da borda em volta, como o cartao do app.
  moldura := TPanel.Create(formEspera);
  moldura.Parent := formEspera;
  moldura.SetBounds(1, 1, formEspera.ClientWidth - 2, formEspera.ClientHeight - 2);
  moldura.BevelOuter := bvNone;
  moldura.Color := corCartao;
  moldura.ParentBackground := False;

  img := TBitmapImage.Create(formEspera);
  img.Parent := moldura;
  img.Bitmap.LoadFromFile(ExpandConstant('{tmp}\espera-icone.bmp'));
  img.SetBounds((moldura.Width - ScaleX(96)) div 2, ScaleY(30), ScaleX(96), ScaleY(96));
  img.Stretch := True;

  titulo := TNewStaticText.Create(formEspera);
  titulo.Parent := moldura;
  titulo.Font.Name := 'Sora SemiBold';
  titulo.Font.Size := 13;
  titulo.Font.Color := corTexto;
  titulo.Caption := 'Atualizando o Hexagon Call';
  titulo.AutoSize := True;
  titulo.Left := (moldura.Width - titulo.Width) div 2;
  titulo.Top := ScaleY(144);

  aviso := TNewStaticText.Create(formEspera);
  aviso.Parent := moldura;
  aviso.Font.Name := 'Space Grotesk';
  aviso.Font.Size := 10;
  aviso.Font.Color := corSec;
  aviso.Caption := 'Instalando a atualização. O app abre sozinho em seguida.';
  aviso.AutoSize := True;
  aviso.Left := (moldura.Width - aviso.Width) div 2;
  aviso.Top := ScaleY(174);

  // Barra de dois paineis: o TNewProgressBar ignora a cor com os temas do
  // Windows ligados. E progresso de verdade: quem move e o
  // CurInstallProgressChanged, conforme o Inno copia.
  trilho := TPanel.Create(formEspera);
  trilho.Parent := moldura;
  trilho.BevelOuter := bvNone;
  trilho.ParentBackground := False;
  trilho.Color := corTrilho;
  trilho.SetBounds(ScaleX(40), ScaleY(200), moldura.Width - ScaleX(80), ScaleY(6));

  barra := TPanel.Create(formEspera);
  barra.Parent := trilho;
  barra.BevelOuter := bvNone;
  barra.ParentBackground := False;
  barra.Color := corVerde;
  barra.SetBounds(0, 0, 0, trilho.Height);

  formEspera.Visible := True;
  // Por cima de tudo, mas sem roubar o foco de quem estiver digitando.
  SetWindowPos(formEspera.Handle, HWND_TOPMOST, 0, 0, 0, 0,
    SWP_NOMOVE or SWP_NOSIZE or SWP_NOACTIVATE);
  // Pintar AGORA, inclusive os filhos: dali o instalador sai direto para
  // descompactar e nao volta a bomba de mensagens por alguns segundos.
  RedrawWindow(formEspera.Handle, 0, 0,
    RDW_INVALIDATE or RDW_UPDATENOW or RDW_ALLCHILDREN);
end;

procedure CurInstallProgressChanged(atual, total: Integer);
begin
  if (formEspera <> nil) and (total > 0) and (trilho <> nil) then
  begin
    barra.Width := (trilho.Width * atual) div total;
    barra.Update;
  end;
end;

// ---------------------------------------------------------------------
// O WEBVIEW2
// ---------------------------------------------------------------------
//
// A janela do app e o WebView2 da Microsoft. Sem ele, o app so mostra um aviso
// em ingles e fecha (vem de dentro do Tauri). O Windows 11 ja traz; no 10 ele
// chega com o Edge, mas pode faltar ou ficar pela metade: visto numa maquina de
// teste em 26/11/2026, com o registro dizendo 153 e a pasta da 153 sem o
// msedgewebview2.exe (uma atualizacao que parou no meio). Por isso a conferencia
// e no ARQUIVO da versao registrada, nao so na chave que a Microsoft documenta.

const
  CHAVE_WEBVIEW2 = 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  URL_WEBVIEW2 = 'https://go.microsoft.com/fwlink/p/?LinkId=2124703';

function WebView2Em(raiz: Integer; chave: String): Boolean;
var
  pv, pasta: String;
begin
  Result := False;
  if not RegQueryStringValue(raiz, chave, 'pv', pv) then
    Exit;
  if (pv = '') or (pv = '0.0.0.0') then
    Exit;
  if not RegQueryStringValue(raiz, chave, 'location', pasta) then
    pasta := ExpandConstant('{commonpf32}\Microsoft\EdgeWebView\Application');
  Result := FileExists(AddBackslash(pasta) + pv + '\msedgewebview2.exe');
  Log('WebView2 ' + pv + ' em ' + pasta + ': ' + IntToStr(Ord(Result)));
end;

function TemWebView2: Boolean;
begin
  Result := WebView2Em(HKLM32, 'SOFTWARE\' + CHAVE_WEBVIEW2) or
    WebView2Em(HKCU, 'Software\' + CHAVE_WEBVIEW2);
end;

// Na instalacao comum, pergunta e roda o instalador oficial da Microsoft (com a
// janela dele: pode pedir administrador para consertar a copia da maquina). Na
// atualizacao silenciosa so registra no log: o app ja estava rodando antes.
procedure ConfereWebView2;
var
  instalador: String;
  codigo: Integer;
begin
  if TemWebView2 then
    Exit;
  Log('WebView2 ausente ou incompleto');
  if WizardSilent then
    Exit;
  if MsgBox('O Hexagon Call precisa do WebView2 da Microsoft para mostrar a janela, e ele não está instalado ' +
      '(ou está incompleto) neste computador.' + #13#10#13#10 + 'Baixar e instalar agora?',
      mbConfirmation, MB_YESNO) <> IDYES then
    Exit;
  try
    DownloadTemporaryFile(URL_WEBVIEW2, 'MicrosoftEdgeWebview2Setup.exe', '', nil);
    instalador := ExpandConstant('{tmp}\MicrosoftEdgeWebview2Setup.exe');
    if not Exec(instalador, '/install', '', SW_SHOWNORMAL, ewWaitUntilTerminated, codigo) or (codigo <> 0) then
      Log('instalador do WebView2 saiu com ' + IntToStr(codigo));
  except
    Log('WebView2: ' + GetExceptionMessage);
  end;
  if not TemWebView2 then
    MsgBox('O WebView2 ainda não está pronto. Sem ele a janela do Hexagon Call não abre.' + #13#10 +
      'Instale pelo site da Microsoft (procure por "WebView2 Runtime") e abra o app de novo.',
      mbError, MB_OK);
end;

procedure CurStepChanged(passo: TSetupStep);
begin
  if passo = ssPostInstall then
  begin
    instalado := True;
    // Antes do [Run]: o app so abre depois de o WebView2 estar no lugar.
    ConfereWebView2;
  end;
end;

function InitializeSetup: Boolean;
begin
  Result := True;
  instalado := False;
  if not WizardSilent then
    Exit;
  // A tela e enfeite; a atualizacao e o que importa. Um tropeco aqui nao pode
  // derrubar o instalador.
  try
    AbrirEspera;
  except
    formEspera := nil;
    Log('tela de espera: falhou, instalando sem ela: ' + GetExceptionMessage);
  end;
end;

procedure DeinitializeSetup;
var
  exe: String;
  codigo: Integer;
begin
  // Atualizacao que falhou no meio (disco cheio, arquivo preso): o app ja tinha
  // saido para dar lugar a ela, e o [Run] nao roda. Reabre o que estiver
  // instalado, para o telefone nao ficar fora do ar sem ninguem saber.
  if WizardSilent and not instalado then
  begin
    try
      exe := ExpandConstant('{app}\HexagonCall.exe');
      if FileExists(exe) then
      begin
        Log('a atualizacao nao terminou: reabrindo ' + exe);
        Exec(exe, '', '', SW_SHOWNORMAL, ewNoWait, codigo);
      end;
    except
      Log('reabrir o app: ' + GetExceptionMessage);
    end;
  end;
  // Fecha no fim de tudo, DEPOIS do [Run] que reabre o app: sem piscar a tela
  // vazia entre a copia acabar e a janela do app subir.
  if formEspera <> nil then
    formEspera.Close;
end;
