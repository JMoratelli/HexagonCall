// Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli). Licenca GNU GPL 3 ou posterior.
//
// A tela. O desenho e o do prototipo (docs/prototipos/softphone.html); os dados vem do telefone em Rust:
//   invoke('estado')            tudo de uma vez, ao abrir
//   invoke('liga' | 'atende' | 'desliga' | 'mudo' | 'espera' | 'mensagem' | 'salva_conta' | ...)
//   listen('telefone')          os avisos: contas, recebendo, tocando, atendida, espera, encerrada, mensagem
// Sem lista de ramais do PBX por enquanto: os nomes vem de quem ligou ou mandou mensagem.
'use strict'

const { invoke } = window.__TAURI__.core
const { listen } = window.__TAURI__.event
const janela = window.__TAURI__.window.getCurrentWindow()

const $ = s => document.querySelector(s)
const esc = s => String(s == null ? '' : s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c])
const norm = s => String(s).normalize('NFD').replace(/\p{M}/gu, '').toLowerCase()
const erroDe = e => String(e && e.message ? e.message : e)

const PHONE = 'M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z'
const MIC = '<rect x="9" y="2" width="6" height="12" rx="3"/><path d="M19 10v1a7 7 0 0 1-14 0v-1M12 18v4"/>'
const DOT = (x, y) => '<circle cx="' + x + '" cy="' + y + '" r="1.6" fill="currentColor" stroke="none"/>'
const ICONS = {
  chat: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>',
  phone: '<path d="' + PHONE + '"/>',
  pin: '<path d="' + PHONE + '"/><path d="M16 2v6h6M22 2l-6 6"/>',
  pout: '<path d="' + PHONE + '"/><path d="M22 8V2h-6M16 8l6-6"/>',
  pmiss: '<path d="' + PHONE + '"/><path d="m22 2-6 6M16 2l6 6"/>',
  hang: '<path transform="rotate(135 12 12)" d="' + PHONE + '"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  baixar: '<path d="M12 3v12M7 10l5 5 5-5M5 21h14"/>',
  send: '<path d="m22 2-7 20-4-9-9-4Z"/><path d="M22 2 11 13"/>',
  mic: MIC,
  micoff: MIC + '<path d="m3 3 18 18"/>',
  pause: '<rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/>',
  keypad: DOT(6, 4) + DOT(12, 4) + DOT(18, 4) + DOT(6, 10) + DOT(12, 10) + DOT(18, 10) + DOT(6, 16) + DOT(12, 16) + DOT(18, 16) + DOT(12, 21.5),
  transfer: '<path d="m16 3 4 4-4 4M20 7H4M8 21l-4-4 4-4M4 17h16"/>',
  back: '<path d="M20 5H9l-7 7 7 7h11a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2Z"/><path d="m18 9-6 6M12 9l6 6"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  alert: '<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4M12 17h.01"/>',
  signal: '<path d="M4 20v-3M9 20v-7M14 20V9M19 20V5"/>',
  caret: '<path d="m6 9 6 6 6-6"/>',
  edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>',
  trash: '<path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  users: '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>',
  bell: '<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  janela: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>',
  play: '<path d="M7 4.5v15l12-7.5z"/>',
  refresh: '<path d="M21 12a9 9 0 1 1-2.64-6.36L21 8"/><path d="M21 3v5h-5"/>',
  min: '<path d="M6 12h12"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>'
}
const ic = (n, cls) => '<svg class="' + (cls || 'ic') + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + ICONS[n] + '</svg>'
// A marca: o hexagono com sinal dos icones (H2 e M2, escolhidos em 26/11), nas cores do flavor.
const MARK = '<svg viewBox="0 0 512 512" aria-hidden="true"><g transform="translate(256 256)">' +
  '<polygon points="-108,-188 108,-188 216,0 108,188 -108,188 -216,0" fill="var(--art-b)" stroke="var(--art-b)" stroke-width="52" stroke-linejoin="round"/>' +
  '<g transform="translate(-118 -96) scale(8)"><path d="' + PHONE + '" fill="#fff" stroke="#fff" stroke-width="1.4" stroke-linejoin="round"/></g>' +
  '<path d="M30 -118a90 90 0 0 1 88 88M30 -64a36 36 0 0 1 34 34" fill="none" stroke="var(--art-a)" stroke-width="30" stroke-linecap="round"/></g></svg>'

const VIEWS = ['chamadas', 'conversas', 'ramais', 'ajustes']
const state = {
  view: 'chamadas', secao: 'contas', rq: '', contas: [], prefs: { flavor: 'hexagon', tema: 'sistema', microfone: null, altoFalante: null, trazerParaFrente: true },
  temChaveiro: true, pasta: '', hist: [], convs: [], nomes: {}, toques: [],
  call: null, panel: 'call', dial: '', dialT: '', dtmf: '', hseg: 'all', missedNew: 0,
  open: null, cq: '', drafts: {}, naoLidas: {},
  form: null, remover: null, devices: null, devErro: null, autostart: null,
  grav: null, retencao: null, ouvindo: null,
  sel: null, apelidos: {}, presencas: {}, editaNome: null,
  // A atualizacao: versao instalada, se este sistema atualiza sozinho, a release achada e a fase
  // (disponivel, baixando, instalando, erro). "Depois" esconde aquela versao por um dia.
  versao: '', atualizaSozinho: false, atu: null, atuDepois: null, procurando: false
}

// ---- Pequenos pedacos ----
const fmt = s => { s = Math.max(0, s | 0); const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), x = String(s % 60).padStart(2, '0'); return h ? h + ':' + String(m).padStart(2, '0') + ':' + x : m + ':' + x }
const hora = t => new Date(t * 1000).toTimeString().slice(0, 5)
const plural = (n, um, varios) => n + ' ' + (n === 1 ? um : varios)
const tamanho = b => b < 1e6 ? Math.max(1, Math.round(b / 1e3)) + ' KB' : (b / 1e6).toLocaleString('pt-BR', { maximumFractionDigits: b < 1e8 ? 1 : 0 }) + ' MB'
const rotuloDias = d => d === 365 ? '1 ano' : d + ' dias'
function dia (t) {
  const d = new Date(t * 1000), hoje = new Date()
  const zero = x => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  const dias = Math.round((zero(hoje) - zero(d)) / 86400000)
  if (dias === 0) return 'Hoje'
  if (dias === 1) return 'Ontem'
  return String(d.getDate()).padStart(2, '0') + '/' + String(d.getMonth() + 1).padStart(2, '0')
}
const finePointer = () => matchMedia('(pointer: fine)').matches
const brandName = () => state.prefs.flavor === 'mcalls' ? 'M Calls' : 'Hexagon Call'
function hue (r) { let h = 0; for (const c of String(r)) h = (h * 31 + c.charCodeAt(0)) % 360; return h }
const ini = n => { const w = n.split(/\s+/).filter(x => /\p{L}/u.test(x) && !/^(da|de|do|dos|das)$/i.test(x)); return (w.length > 1 ? w[0][0] + w[w.length - 1][0] : (w[0] || n).slice(0, 2)).toUpperCase() }

/** O nome de um ramal: o que veio na ligacao ou na mensagem (o PBX manda no From). */
function nomeDe (r) {
  if (state.apelidos[r]) return state.apelidos[r]
  if (state.nomes[r]) return state.nomes[r]
  for (let i = state.hist.length - 1; i >= 0; i--) if (state.hist[i].ramal === r && state.hist[i].nome) return state.hist[i].nome
  for (let i = state.convs.length - 1; i >= 0; i--) if (state.convs[i].ramal === r && state.convs[i].nome) return state.convs[i].nome
  return null
}
const pessoa = r => { const n = nomeDe(r); return { r, n: n || r, conhecido: !!n, h: hue(r) } }
const primeiro = d => d.conhecido ? d.n.split(' ')[0] : d.r
const av = (d, size) => '<span class="av' + (size ? ' ' + size : '') + '" style="--h:' + d.h + '" aria-hidden="true">' + esc(d.conhecido ? ini(d.n) : '#') + '</span>'
const ehNumero = v => /^[0-9*#+]+$/.test(v)
const contaPrincipal = () => state.contas.find(c => c.principal) || state.contas.find(c => c.registro === 'registrado') || state.contas[0] || null
const ramalDaConta = id => String(id || '').split('@')[0]
/** A conta da ligacao que vai sair: a escolhida na setinha (so ate ligar), ou a principal. */
const contaDaLigacao = () => state.contas.find(c => c.id === state.contaLigacao) || contaPrincipal()
const meusRamais = () => new Set(state.contas.map(c => c.usuario))

/** Quem ja ligou ou mandou mensagem, do mais recente para o mais antigo. */
function conhecidos () {
  const vistos = new Map()
  const todos = [...state.hist.map(h => [h.quando, h.ramal]), ...state.convs.map(c => [c.quando, c.ramal])].sort((a, b) => b[0] - a[0])
  for (const [, r] of todos) if (!vistos.has(r) && !meusRamais().has(r)) vistos.set(r, pessoa(r))
  return [...vistos.values()]
}

const REGISTRO = {
  registrado: ['Registrado', 'var(--ok)'],
  iniciando: ['Conectando', 'var(--info)'],
  falhou: ['Falhou', 'var(--crit)'],
  'sem-registro': ['Sem registro', 'var(--muted-fg)'],
  'sem-senha': ['Sem senha', 'var(--warn)'],
  parada: ['Parada', 'var(--muted-fg)']
}
// O estado da conta como a tela mostra: o registro e a sonda do PBX (OPTIONS a cada 30 s) juntos. Sem resposta
// do PBX (a sonda, ou o registro sem resposta) e ambar e o app tenta sozinho; vermelho so quando o PBX recusa.
const reconectando = c => !!c.foraDesde && (c.registro === 'iniciando' || (c.registro === 'falhou' && c.falhaDeRede))
function estadoConta (c) {
  if (c.registro === 'registrado' && c.semResposta) return ['Sem resposta do PBX', 'var(--warn)']
  if (reconectando(c)) return ['Sem conexão com o PBX', 'var(--warn)']
  return REGISTRO[c.registro] || REGISTRO.parada
}
const CURTO = { iniciando: 'conectando', falhou: 'falhou', 'sem-senha': 'sem senha', parada: 'parada' }
const curtoConta = c => c.registro === 'registrado' && c.semResposta ? 'sem resposta' : reconectando(c) ? 'reconectando' : CURTO[c.registro] || ''
function pbxTexto (c) {
  if (reconectando(c) || (c.registro === 'registrado' && c.semResposta)) return 'sem resposta desde ' + hora(c.foraDesde) + '; tenta de novo sozinho a cada 30 s'
  if (c.pbxMs == null || c.pbxEm == null) return ''
  const s = Math.max(0, Math.round(Date.now() / 1000 - c.pbxEm))
  return 'o PBX respondeu em ' + c.pbxMs + ' ms, há ' + (s < 90 ? s + ' s' : Math.round(s / 60) + ' min')
}
const regPill = c => {
  const [t, cor] = estadoConta(c), extra = [c.detalhe, pbxTexto(c)].filter(Boolean).join(' · ')
  const ms = c.registro === 'registrado' && !c.semResposta && c.pbxMs != null ? ' · ' + c.pbxMs + ' ms' : ''
  return '<span class="pill" style="--c:' + cor + '"' + (extra ? ' title="' + esc(extra) + '"' : '') + '><span class="dot"></span>' + t + ms + (c.detalhe ? ' · ' + esc(c.detalhe) : '') + '</span>'
}

// ---- A barra do app: marca, abas, registro e engrenagem ----
function renderChrome () {
  const raiz = document.documentElement
  raiz.dataset.scheme = state.prefs.flavor === 'mcalls' ? 'red' : 'green'
  if (state.prefs.tema === 'escuro') raiz.dataset.theme = 'dark'
  else if (state.prefs.tema === 'claro') raiz.dataset.theme = 'light'
  else delete raiz.dataset.theme
  document.title = brandName()
  const unread = Object.values(state.naoLidas).reduce((a, b) => a + b, 0)
  const c = contaPrincipal()
  let status
  if (!c) status = '<button type="button" class="status" style="--c:var(--warn)" data-act="nova-conta" title="Cadastre o seu ramal"><span class="dot"></span>Sem conta</button>'
  else {
    // O botao do status (27/11) abre o Disponivel/Nao perturbe, a conexao de cada conta e o Reconectar. Um problema
    // da conta tem a vez no texto; o nao perturbe fica sempre na barrinha do ponto.
    const [t, cor] = estadoConta(c), curto = curtoConta(c), dnd = emNaoPerturbe()
    status = '<button type="button" class="status" style="--c:' + (dnd && !curto ? 'var(--crit)' : cor) + '" data-act="status" data-pop aria-haspopup="dialog" aria-expanded="false" title="' + esc([c.id, t + (c.detalhe ? ': ' + c.detalhe : ''), pbxTexto(c), dnd ? 'não perturbe' : ''].filter(Boolean).join(' · ')) + '"><span class="dot' + (dnd ? ' dnd' : '') + '"></span>' + esc(c.usuario) + (curto ? ' · ' + curto : dnd ? ' · não perturbe' : '') + (state.contas.length > 1 ? ' +' + (state.contas.length - 1) : '') + '</button>'
  }
  // A barra de cima e a da janela: marca, conta e os botoes (arrasta pelo vazio).
  $('#abar').innerHTML = '<span class="brand" data-tauri-drag-region>' + MARK + '<span>' + brandName() + '</span></span>' + status +
    '<span class="wbtns"><button type="button" class="iconbtn ghost sm" data-act="minimizar" aria-label="Minimizar" title="Minimizar">' + ic('min') + '</button>' +
    '<button type="button" class="iconbtn ghost sm fecha" data-act="fechar" aria-label="Fechar (continua na bandeja)" title="Fechar (continua na bandeja)">' + ic('x') + '</button></span>'
  // As secoes no trilho da esquerda (pedido de 26/11); as configuracoes no pe.
  const botao = (v, label, icone, n, tom) => '<button type="button" data-act="go" data-v="' + v + '"' + (state.view === v ? ' aria-current="page"' : '') + ' title="' + label + '">' +
    '<span class="icone">' + ic(icone) + (n ? '<span class="n" style="--c:' + tom + '">' + n + '</span>' : '') + '</span><span>' + label + '</span></button>'
  $('#rail').innerHTML = botao('chamadas', 'Chamadas', 'phone', state.missedNew, 'var(--crit)') + botao('conversas', 'Conversas', 'chat', unread, 'var(--primary)') + botao('ramais', 'Ramais', 'users') +
    '<span class="espaco"></span>' + botao('ajustes', 'Ajustes', 'gear', contaComProblema() && !oculta() ? '!' : 0, 'var(--warn)')
}

// ---- A faixa da ligacao (nas abas que nao sao Chamadas) ----
function renderStrip () {
  const el = $('#strip'), c = state.call
  const show = !!c && state.view !== 'chamadas'
  el.hidden = !show
  if (!show) { el.innerHTML = ''; return }
  const d = pessoa(c.r)
  let tx, ctl
  if (c.kind === 'incoming') {
    tx = ic('pin') + '<span class="tx"><b>' + esc(d.n) + '</b> <span>' + esc(c.r) + ' · chamando você</span></span>'
    ctl = '<button type="button" class="cbtn hang" data-act="decline">' + ic('hang') + 'Recusar</button><button type="button" class="cbtn answer" data-act="answer">' + ic('phone') + 'Atender</button>'
  } else if (c.kind === 'outgoing') {
    tx = ic('pout') + '<span class="tx"><b>' + esc(d.n) + '</b> <span>' + esc(c.r) + (c.tocando ? ' · tocando…' : ' · chamando…') + '</span></span>'
    ctl = '<button type="button" class="cbtn hang" data-act="hangup">' + ic('hang') + 'Cancelar</button>'
  } else {
    tx = ic('phone') + '<span class="tx"><b>' + esc(d.n) + '</b> <span>' + esc(c.r) + (c.held ? ' · em espera' : c.deles ? ' · ' + esc(primeiro(d)) + ' pôs em espera' : '') + '</span>' + (c.semSom ? ' <span class="nao-ouco">não te ouço</span>' : '') + '</span><span class="strip-timer" id="strip-timer">' + fmt((Date.now() - c.t0) / 1000) + '</span>'
    ctl = '<button type="button" class="cbtn" data-act="mute" aria-pressed="' + !!c.muted + '">' + ic(c.muted ? 'micoff' : 'mic') + 'Mudo</button><button type="button" class="cbtn" data-act="go" data-v="chamadas">Abrir</button><button type="button" class="cbtn hang" data-act="hangup">' + ic('hang') + 'Desligar</button>'
  }
  el.innerHTML = tx + ctl
}

// ---- Chamadas ----
function renderChamadas () {
  state.missedNew = 0
  if (!state.contas.length) {
    $('#content').innerHTML = '<div class="boasvindas"><div class="lattice" aria-hidden="true"></div><span class="marca">' + MARK + '</span><h2>Cadastre o seu ramal para começar</h2>' +
      '<p>O ramal, o servidor do PBX e a senha. A senha fica no chaveiro do sistema' + (state.temChaveiro ? '' : ' (nesta máquina não há chaveiro: ela fica num arquivo só seu)') + '.</p>' +
      '<button type="button" class="btn prim" data-act="nova-conta">' + ic('plus') + 'Cadastrar conta</button></div>'
    return
  }
  $('#content').innerHTML = '<div class="calls"><section class="lpanel" id="lpanel" aria-label="Teclado"></section><section class="recent" id="recent" aria-label="Últimas chamadas"></section></div>'
  renderLeft(); renderRecent()
}
function renderLeft () {
  const el = $('#lpanel'); if (!el) return
  const c = state.call
  let html
  if (!c) html = dialPanel()
  else if (c.kind === 'incoming') html = incomingPanel(c)
  else if (c.kind === 'active' && state.panel === 'dtmf') html = dtmfPanel(c)
  else if (c.kind === 'active' && state.panel === 'transfer') html = transferPanel(c)
  else html = callPanel(c)
  el.innerHTML = '<div class="lattice" aria-hidden="true"></div>' + html
  focusDial()
}
function focusDial () {
  if (!finePointer() || state.view !== 'chamadas') return
  const inp = $('#dial-main') || $('#dial-transfer')
  if (inp && document.activeElement !== inp) { inp.focus({ preventScroll: true }); const n = inp.value.length; inp.setSelectionRange(n, n) }
}
const KEYS = [['1', ''], ['2', 'ABC'], ['3', 'DEF'], ['4', 'GHI'], ['5', 'JKL'], ['6', 'MNO'], ['7', 'PQRS'], ['8', 'TUV'], ['9', 'WXYZ'], ['*', ''], ['0', '+'], ['#', '']]
const padHTML = m => '<div class="pad">' + KEYS.map(([k, l]) => '<button type="button" class="key" data-act="dialkey" data-m="' + (m || 'main') + '" data-v="' + k + '" aria-label="' + k + '" tabindex="-1"><b>' + k + '</b><small>' + l + '</small></button>').join('') + '</div>'
function dialPanel () {
  return '<label class="dial-disp"><input id="dial-main" type="text" autocomplete="off" spellcheck="false" aria-label="Ramal, número ou nome" placeholder="Ramal, número ou nome" value="' + esc(state.dial) + '">' +
    '<button type="button" class="iconbtn sm" data-act="dialback" aria-label="Apagar o último" title="Apagar" tabindex="-1">' + ic('back') + '</button></label>' +
    '<div class="match" id="match-main">' + matchHTML() + '</div>' + padHTML('main') + ligarHTML()
}
function ligarHTML () {
  if (state.contas.length < 2) return '<button type="button" class="callbtn" data-act="dialgo">' + ic('phone') + 'Ligar</button>'
  const c = contaDaLigacao(), outra = c && !c.principal && state.contaLigacao
  return '<div class="ligar"><button type="button" class="callbtn" data-act="dialgo">' + ic('phone') + 'Ligar' + (outra ? ' <small>· ' + esc(c.usuario) + '</small>' : '') + '</button>' +
    '<button type="button" class="callbtn seta" data-act="pick-conta" data-pop aria-haspopup="dialog" aria-expanded="false" aria-label="Ligar por outra conta" title="Ligar por ' + esc(c ? c.usuario : '') + ' (trocar)">' + ic('caret') + '</button></div>'
}
function matches () {
  const v = state.dial.trim(); if (!v) return []
  const lista = conhecidos()
  return ehNumero(v) ? lista.filter(d => d.r.startsWith(v)) : lista.filter(d => d.conhecido && norm(d.n).includes(norm(v)))
}
function matchHTML () {
  const v = state.dial.trim()
  if (!v) return '<p class="mhint">Digite o ramal ou o número. <kbd>Enter</kbd> liga.</p>'
  const list = matches().slice(0, 3)
  if (!list.length) return '<p class="mhint">' + (ehNumero(v) ? '<kbd>Enter</kbd> liga para ' + esc(v) + '.' : 'Ninguém com esse nome por aqui. Digite o ramal.') + '</p>'
  return list.map((d, i) => '<button type="button" class="opt" data-act="dialpick" data-v="' + esc(d.r) + '"' + (i === 0 ? ' data-first="1"' : '') + ' title="Ligar para ' + esc(d.n) + '" tabindex="-1">' + av(d, 'sm') + '<span class="nm">' + esc(d.n) + '</span><span class="ct">' + esc(d.r) + '</span>' + (i === 0 && !ehNumero(v) ? '<kbd>Enter</kbd>' : '') + '</button>').join('')
}
function renderMatch () { const el = $('#match-main'); if (el) el.innerHTML = matchHTML() }
function dialTarget () {
  const v = state.dial.trim()
  if (!v) return null
  if (ehNumero(v)) return v
  const m = matches()[0]
  return m ? m.r : null
}
function callPanel (c) {
  const d = pessoa(c.r)
  const who = '<div class="cp-who">' + av(d, 'xl') + '<b>' + esc(d.n) + '</b><small>' + esc(c.r) + '</small></div>'
  if (c.kind === 'outgoing') {
    return '<div class="cp">' + who + '<span class="cp-state">' + ic('pout') + (c.tocando ? 'Tocando…' : 'Chamando…') + '</span></div><button type="button" class="hangbtn" data-act="hangup">' + ic('hang') + 'Cancelar</button>'
  }
  const notas = (c.gravando ? ' · <b class="rec">gravando</b>' : '') + (c.held ? ' · <b>em espera</b>' : '') + (c.deles ? ' · <b>' + esc(primeiro(d)) + ' pôs em espera</b>' : '') + (c.muted ? ' · <b>mudo</b>' : '')
  const outra = (c.alternativas || [])[0]
  const trocar = '<button type="button" class="btn sm" data-act="pick" data-k="microfoneLigacao" data-pop aria-haspopup="dialog" aria-expanded="false" title="Microfone">' + (outra ? 'Outro' : 'Trocar') + '</button>'
  let alerta = ''
  if (c.semMic) alerta = '<div class="alerta" role="status">' + ic('micoff') + '<div><b>' + (c.semFone ? 'Sem microfone e sem alto-falante' : 'Sem microfone') + '</b><span>' + (c.semFone ? 'Nenhum dispositivo de som abriu: ninguém se ouve. Conecte um headset e escolha o microfone.' : 'Nenhum microfone abriu: o outro lado não te ouve. Conecte um e escolha aqui.') + '</span></div>' + trocar + '</div>'
  else if (c.semFone) alerta = '<div class="alerta" role="status">' + ic('micoff') + '<div><b>Sem alto-falante</b><span>Nenhuma saída de som abriu: você não ouve o outro lado.</span></div></div>'
  else if (c.semSom && c.micMudo) alerta = '<div class="alerta" role="status">' + ic('micoff') + '<div><b>Seu microfone está mudo</b><span>Está mudo no sistema (ou com o volume em zero).</span></div><button type="button" class="btn sm prim" data-act="tira-do-mudo">Tirar do mudo</button></div>'
  else if (c.semSom && outra) alerta = '<div class="alerta" role="status">' + ic('micoff') + '<div><b>Não te ouço</b><span>' + esc(outra.nome) + ' está captando som. Usar esse?</span></div><button type="button" class="btn sm prim" data-act="usa-microfone" data-v="' + esc(outra.id) + '">Usar</button>' + trocar + '</div>'
  else if (c.semSom) alerta = '<div class="alerta" role="status">' + ic('micoff') + '<div><b>Não te ouço</b><span>O microfone está mandando silêncio. Quer testar outro?</span></div>' + trocar + '</div>'
  return '<div class="cp">' + who + '<div><div class="cp-timer" id="cp-timer">' + fmt((Date.now() - c.t0) / 1000) + '</div><div class="cp-q">' + ic('signal') + esc(c.codec || 'G.711') + ' · 8 kHz' + notas + '</div></div>' + alerta +
    '<div class="cp-grid">' +
    '<button type="button" class="cpb" data-act="mute" aria-pressed="' + !!c.muted + '">' + ic(c.muted ? 'micoff' : 'mic') + 'Mudo</button>' +
    '<button type="button" class="cpb" data-act="hold" aria-pressed="' + !!c.held + '">' + ic('pause') + 'Espera</button>' +
    '<button type="button" class="cpb" data-act="panel" data-v="dtmf">' + ic('keypad') + 'Teclado</button>' +
    '<button type="button" class="cpb" data-act="panel" data-v="transfer">' + ic('transfer') + 'Transferir</button></div></div>' +
    '<button type="button" class="hangbtn" data-act="hangup">' + ic('hang') + 'Desligar</button>'
}
function dtmfPanel (c) {
  return '<div class="phead"><b>Teclado · ' + esc(primeiro(pessoa(c.r))) + '</b><span class="muted tab" id="cp-timer">' + fmt((Date.now() - c.t0) / 1000) + '</span></div>' +
    '<label class="dial-disp"><input id="dtmf-disp" type="text" readonly tabindex="-1" aria-label="Tons enviados" placeholder="Tons vão para a ligação" value="' + esc(state.dtmf) + '"></label>' +
    '<p class="mhint">Para menu de atendimento, senha ou ramal de destino. O teclado do computador também envia.</p>' + padHTML('dtmf') +
    '<div class="two"><button type="button" class="ghostbtn" data-act="panel" data-v="call">Voltar</button><button type="button" class="hangbtn" data-act="hangup">' + ic('hang') + 'Desligar</button></div>'
}
function transferPanel (c) {
  const d = pessoa(c.r)
  return '<div class="phead"><b>Transferir ' + esc(primeiro(d)) + ' para</b><span class="muted tab" id="cp-timer">' + fmt((Date.now() - c.t0) / 1000) + '</span></div>' +
    '<label class="dial-disp"><input id="dial-transfer" type="text" autocomplete="off" spellcheck="false" aria-label="Ramal para transferir" placeholder="Ramal ou número" value="' + esc(state.dialT) + '">' +
    '<button type="button" class="iconbtn sm" data-act="dialback" data-m="transfer" aria-label="Apagar o último" title="Apagar" tabindex="-1">' + ic('back') + '</button></label>' +
    '<p class="mhint">A ligação vai direto para o ramal digitado, e a sua sai (transferência sem consulta).</p>' + padHTML('transfer') +
    '<div class="two"><button type="button" class="ghostbtn" data-act="panel" data-v="call">Voltar</button><button type="button" class="callbtn" data-act="transfere">' + ic('transfer') + 'Transferir</button></div>'
}
function incomingPanel (c) {
  const d = pessoa(c.r)
  const para = state.contas.length > 1 && c.conta ? ' · para ' + esc(ramalDaConta(c.conta)) : ''
  return '<div class="cp ringing"><div class="cp-who">' + av(d, 'xl') + '<b>' + esc(d.n) + '</b><small>' + esc(c.r) + para + '</small></div><span class="cp-state">' + ic('pin') + 'Chamada recebida</span></div>' +
    '<div class="two"><button type="button" class="hangbtn" data-act="decline">' + ic('hang') + 'Recusar</button><button type="button" class="answerbtn" data-act="answer">' + ic('phone') + 'Atender</button></div>'
}
// O texto do fim vem do nucleo (Fim::texto): "Ligação transferida" e o da transferencia que se completou,
// "Atendida em outro ramal" o de quando tocou aqui e em outros aparelhos (grupo, fila) e outro atendeu.
const transferida = h => h.fim === 'Ligação transferida'
const emOutroRamal = h => h.fim === 'Atendida em outro ramal'
const peloNaoPerturbe = h => h.fim === 'Não perturbe'
const perdida = h => h.entrada && !h.atendida && !emOutroRamal(h)
function histTipo (h) {
  if (perdida(h)) return h.fim === 'Ligação recusada' ? 'Recusada' : peloNaoPerturbe(h) ? 'Não perturbe' : 'Perdida'
  if (emOutroRamal(h)) return 'Atendida em outro ramal'
  if (!h.atendida) return 'Não completada'
  if (transferida(h)) return 'Transferida'
  return h.entrada ? 'Recebida' : 'Feita'
}
const histIcone = h => perdida(h) ? ['pmiss', 'var(--crit)'] : emOutroRamal(h) ? ['pin', 'var(--warn)'] : h.entrada ? ['pin', 'var(--ok)'] : ['pout', 'var(--info)']
// Excluir do historico: o modo de selecao (a lixeira no cabecalho) marca as ligacoes; a confirmacao pergunta se
// apaga tambem as gravacoes delas (mockup aprovado em 27/11).
// A linha toda, menos o nome e o fim: so a hora, a conta e o ramal pegariam duas ligacoes do mesmo segundo.
const chaveHist = h => [h.quando, h.conta, h.ramal, h.entrada, h.atendida, h.duracao, h.gravacao || ''].join('|')
function renderRecent () {
  const el = $('#recent'); if (!el) return
  const sel = state.sel
  const todas = [...state.hist].sort((x, y) => y.quando - x.quando)
  const missed = todas.filter(perdida).length
  const rows = todas.filter(h => state.hseg === 'all' || perdida(h)).slice(0, 200)
  // Com alguma gravacao na lista, toda linha reserva o lugar do botao de ouvir: as horas ficam alinhadas.
  const comOuvir = !sel && rows.some(h => h.gravacao)
  let grupo = '', body = ''
  for (const h of rows) {
    const g = dia(h.quando)
    if (g !== grupo) { grupo = g; body += '<div class="rgrp">' + esc(g) + '</div>' }
    const d = pessoa(h.ramal), [icon, c] = histIcone(h), k = chaveHist(h), marcada = !!sel && sel.marcadas.has(k)
    const sub = esc(h.ramal) + ' · <span class="k">' + histTipo(h) + '</span>' + (h.duracao ? ' · ' + fmt(h.duracao) : '') + (!h.atendida && !perdida(h) && !emOutroRamal(h) ? ' · ' + esc(h.fim) : '') + (state.contas.length > 1 ? ' · ' + (h.entrada ? 'para ' : 'pelo ') + esc(ramalDaConta(h.conta)) : '') + (h.gravacao ? ' ' + ic('mic', 'ic gr') : '')
    const marca = sel ? '<span class="ckbox' + (marcada ? ' on' : '') + '" aria-hidden="true">' + ic('check') + '</span>' : '<span class="dir" style="--c:' + c + '">' + ic(icon) + '</span>'
    const acao = sel ? ' data-act="marca" data-v="' + esc(k) + '" role="checkbox" aria-checked="' + marcada + '"' : ' data-act="fill" data-v="' + esc(h.ramal) + '" title="Pôr ' + esc(h.ramal) + ' no campo"'
    body += '<div class="hrow' + (marcada ? ' marcada' : '') + '"><button type="button" class="hmain' + (perdida(h) ? ' missed' : emOutroRamal(h) ? ' outro' : '') + '"' + acao + '>' +
      marca + '<span class="tx"><b>' + esc(d.n) + '</b><small>' + sub + '</small></span><span class="when">' + hora(h.quando) + '</span></button>' +
      (sel ? '' : (h.gravacao ? '<button type="button" class="iconbtn sm" data-act="ouvir-gravacao" data-v="' + esc(h.gravacao) + '" aria-label="Ouvir a gravação com ' + esc(d.n) + '" title="Ouvir a gravação">' + ic('play') + '</button>' : comOuvir ? '<span class="iconbtn sm oco" aria-hidden="true"></span>' : '') +
        '<button type="button" class="iconbtn sm go" data-act="call" data-v="' + esc(h.ramal) + '" aria-label="Ligar para ' + esc(d.n) + '" title="Ligar">' + ic('phone') + '</button>' +
        '<button type="button" class="iconbtn sm" data-act="msg" data-v="' + esc(h.ramal) + '" aria-label="Mensagem para ' + esc(d.n) + '" title="Mensagem">' + ic('chat') + '</button>') + '</div>'
  }
  const vazio = state.hseg === 'all'
    ? '<div class="empty"><b>Nenhuma ligação ainda</b><span>As ligações feitas e recebidas neste computador aparecem aqui.</span></div>'
    : '<div class="empty"><b>Nenhuma perdida</b><span>Quando alguém ligar e você não atender, aparece aqui.</span></div>'
  const marcadas = sel ? state.hist.filter(h => sel.marcadas.has(chaveHist(h))) : []
  const n = marcadas.length
  const cabeca = sel
    ? '<header class="rhead"><h2>' + (n ? plural(n, 'selecionada', 'selecionadas') : 'Toque nas ligações') + '</h2><div class="selacts">' +
      '<button type="button" class="btn sm" data-act="marca-todas">' + (n && n === rows.length ? 'Desmarcar' : 'Marcar todas') + '</button>' +
      '<button type="button" class="btn sm perigo" data-act="exclui-hist"' + (n ? '' : ' disabled') + '>' + ic('trash') + 'Excluir</button>' +
      '<button type="button" class="btn sm" data-act="sel-sai">Cancelar</button></div></header>'
    : '<header class="rhead"><h2>Últimas chamadas</h2><div class="rtools"><div class="mseg" role="group" aria-label="Filtro">' +
      '<button type="button" data-act="hseg" data-v="all" aria-pressed="' + (state.hseg === 'all') + '">Todas</button>' +
      '<button type="button" data-act="hseg" data-v="missed" aria-pressed="' + (state.hseg === 'missed') + '">Perdidas <span class="n">' + missed + '</span></button></div>' +
      (state.hist.length ? '<button type="button" class="iconbtn sm" data-act="sel-entra" aria-label="Selecionar para excluir" title="Selecionar para excluir">' + ic('trash') + '</button>' : '') + '</div></header>'
  const gravadas = marcadas.filter(h => h.gravacao).length
  const confirma = sel && sel.confirma
    ? '<div class="confirma" role="alert"><span>Excluir ' + plural(n, 'ligação', 'ligações') + ' do histórico?' +
      (gravadas ? '<label class="junto"><input type="checkbox" id="exclui-gravacoes"' + (sel.gravacoes ? ' checked' : '') + '> Apagar também ' + (gravadas === 1 ? 'a gravação dela' : 'as ' + gravadas + ' gravações delas') + '</label>' : '') + '</span>' +
      '<button type="button" class="btn" data-act="exclui-nao">Cancelar</button><button type="button" class="btn perigo" data-act="exclui-sim">Excluir</button></div>'
    : ''
  el.innerHTML = cabeca + confirma + '<div class="rlist">' + (body || vazio) + '</div>'
}

// ---- Conversas ----
function threads () {
  const ultimo = new Map()
  for (const m of state.convs) ultimo.set(m.ramal, m)
  if (state.open && !ultimo.has(state.open)) ultimo.set(state.open, null)
  return [...ultimo.entries()].sort((a, b) => (b[1] ? b[1].quando : Infinity) - (a[1] ? a[1].quando : Infinity))
}
function renderConversas () {
  if (!state.open) { const t = threads()[0]; state.open = t ? t[0] : null }
  if (state.open) delete state.naoLidas[state.open]
  $('#content').innerHTML = '<div class="split" id="split">' +
    '<div class="clist-wrap"><div class="clist-head"><label class="search">' + ic('search') + '<input id="cq" type="search" placeholder="Buscar ou ramal novo" autocomplete="off" aria-label="Buscar conversa ou digitar um ramal" value="' + esc(state.cq) + '"></label></div><div class="clist" id="clist"></div></div>' +
    '<div class="thread" id="thread"></div></div>'
  renderClist(); renderThread()
}
function renderClist () {
  const q = norm(state.cq.trim())
  const list = threads().filter(([r]) => { const d = pessoa(r); return !q || norm(d.n).includes(q) || r.includes(q) })
  let html = list.map(([r, m]) => {
    const d = pessoa(r), n = state.naoLidas[r] || 0
    const pv = m ? '<span>' + (m.recebida ? '' : 'Você: ') + esc(m.texto) + '</span>' : '<span>Conversa nova</span>'
    return '<button type="button" class="citem' + (n ? ' unread' : '') + '" data-act="open" data-v="' + esc(r) + '"' + (r === state.open ? ' aria-current="true"' : '') + '>' + av(d) +
      '<span class="tx"><span class="nm">' + esc(d.n) + '</span><span class="pv">' + pv + '</span></span>' +
      '<span class="meta"><span>' + (m ? (dia(m.quando) === 'Hoje' ? hora(m.quando) : dia(m.quando)) : '') + '</span>' + (n ? '<span class="badge">' + n + '</span>' : '') + '</span></button>'
  }).join('')
  const v = state.cq.trim()
  if (v && ehNumero(v) && !list.some(([r]) => r === v)) html = '<button type="button" class="citem" data-act="open" data-v="' + esc(v) + '">' + av(pessoa(v)) + '<span class="tx"><span class="nm">Conversa com ' + esc(v) + '</span><span class="pv"><span>Enter abre</span></span></span></button>' + html
  $('#clist').innerHTML = html || '<div class="empty"><b>Nenhuma conversa</b><span>Digite um ramal na busca para começar uma.</span></div>'
}
function threadHTML (r) {
  const items = state.convs.filter(m => m.ramal === r)
  const d = pessoa(r)
  if (!items.length) return '<p class="t-empty">Nenhuma mensagem com ' + esc(primeiro(d)) + ' ainda. O PBX não guarda mensagens: o que vocês trocarem fica neste computador.</p>'
  let out = '', grupo = ''
  items.forEach((m, i) => {
    const g = dia(m.quando)
    if (g !== grupo) { grupo = g; out += '<div class="day-sep">' + esc(g) + '</div>' }
    const same = x => x && x.recebida === m.recebida && dia(x.quando) === g
    const prev = same(items[i - 1]), next = same(items[i + 1])
    const pos = prev ? (next ? 'mid' : 'last') : (next ? 'first' : 'single')
    const meta = next ? '' : '<span class="mt"' + (m.recebida ? '' : ' title="O PBX aceitou a mensagem (202). O SIP não confirma a entrega."') + '>' + hora(m.quando) + (m.recebida ? '' : ' · ' + ic('check') + 'aceita') + '</span>'
    out += '<div class="msg ' + (m.recebida ? 'l' : 'r') + ' run-' + pos + '"><div class="bub">' + esc(m.texto) + '</div>' + meta + '</div>'
  })
  return out
}
function renderThread () {
  const el = $('#thread'); if (!el) return
  const r = state.open
  if (!r) { el.innerHTML = '<p class="t-empty">Escolha uma conversa, ou digite um ramal na busca.</p>'; return }
  const d = pessoa(r)
  el.innerHTML =
    '<header class="th-head">' + av(d) + '<div class="tx"><b>' + esc(d.n) + '</b><small><span class="tab">' + esc(r) + '</span></small></div>' +
    '<button type="button" class="iconbtn go" data-act="call" data-v="' + esc(r) + '" aria-label="Ligar para ' + esc(d.n) + '" title="Ligar">' + ic('phone') + '</button></header>' +
    '<div class="tview"><div class="lattice" aria-hidden="true"></div><div class="tbody" id="tbody" role="log" aria-label="Mensagens com ' + esc(d.n) + '">' + threadHTML(r) + '</div></div>' +
    '<form class="composer" id="cform"><textarea id="msg-in" rows="1" placeholder="Mensagem para ' + esc(primeiro(d)) + ' (' + esc(r) + ')" aria-label="Mensagem para ' + esc(d.n) + '">' + esc(state.drafts[r] || '') + '</textarea>' +
    '<button type="submit" class="sendbtn" id="sendbtn" aria-label="Enviar" title="Enviar (Enter)"' + (state.drafts[r] ? '' : ' disabled') + '>' + ic('send') + '</button></form>'
  scrollThread(); autosize()
}
function scrollThread () { const b = $('#tbody'); if (b) b.scrollTop = b.scrollHeight }
function autosize () { const t = $('#msg-in'); if (!t) return; t.style.height = '36px'; t.style.height = Math.min(t.scrollHeight + 2, 110) + 'px' }
async function send () {
  const ta = $('#msg-in'); if (!ta) return
  const texto = ta.value.trim(); if (!texto) return
  const r = state.open
  $('#sendbtn').disabled = true
  try {
    const ultima = [...state.convs].reverse().find(m => m.ramal === r)
    const item = await invoke('mensagem', { destino: r, texto, conta: ultima && state.contas.some(c => c.id === ultima.conta) ? ultima.conta : null })
    state.convs.push(item); state.drafts[r] = ''
    if (state.view === 'conversas' && state.open === r) { renderThread(); renderClist(); const t = $('#msg-in'); if (t) t.focus() }
  } catch (e) {
    toast('A mensagem não saiu: ' + erroDe(e)); $('#sendbtn').disabled = false
  }
}

// ---- Ramais ----
function renderRamais () {
  $('#content').innerHTML = '<div class="tools"><label class="search">' + ic('search') + '<input id="rq" type="search" placeholder="Buscar nome ou ramal" autocomplete="off" aria-label="Buscar nome ou ramal" value="' + esc(state.rq) + '"></label></div>' +
    '<div class="dlist" id="dlist"></div>' +
    '<p class="rodape muted">A lista completa, com o nome cadastrado no PBX, vem numa próxima versão. Por enquanto aparece quem já ligou ou mandou mensagem.</p>'
  renderDlist()
  if (finePointer()) { const i = $('#rq'); if (i) { i.focus({ preventScroll: true }); const n = i.value.length; i.setSelectionRange(n, n) } }
}
// Presenca (o PBX diz pela assinatura de cada ramal) e o nome dado aqui (o lapis; Enter salva, Esc cancela).
const PRESENCA = { livre: ['Livre', 'var(--ok)'], ocupado: ['Em ligação', 'var(--crit)'], tocando: ['Tocando', 'var(--warn)'], offline: ['Offline', 'var(--muted-fg)'] }
function comPresenca (d, size) {
  const p = PRESENCA[state.presencas[d.r]], a = av(d, size)
  return p ? a.replace(/<\/span>$/, '<span class="pr" style="--c:' + p[1] + '" title="' + p[0] + '"></span></span>') : a
}
function renderDlist () {
  const el = $('#dlist'); if (!el) return
  const q = norm(state.rq.trim())
  const lista = conhecidos().filter(d => !q || norm(d.n).includes(q) || d.r.includes(q))
  const linha = d => {
    const p = PRESENCA[state.presencas[d.r]]
    if (state.editaNome === d.r) {
      return '<div class="drow editando">' + comPresenca(d) + '<div class="tx"><input class="nome-ed" id="nome-ed" maxlength="60" value="' + esc(state.apelidos[d.r] || (d.conhecido ? d.n : '')) + '" placeholder="Nome para o ' + esc(d.r) + '" aria-label="Nome do ramal ' + esc(d.r) + '">' +
        '<small>Ramal ' + esc(d.r) + ' · o nome fica só neste computador</small></div><span></span><div class="acts">' +
        '<button type="button" class="btn sm" data-act="nome-nao">Cancelar</button><button type="button" class="btn sm prim" data-act="nome-sim" data-v="' + esc(d.r) + '">Salvar</button></div></div>'
    }
    return '<div class="drow">' + comPresenca(d) + '<div class="tx"><b>' + esc(d.n) + '</b><small>' + (p ? '<span class="pres" style="--c:' + p[1] + '">' + p[0] + '</span>' : '') + (d.conhecido ? esc(d.r) : 'sem nome ainda') + '</small></div>' +
      '<span class="rnum">' + esc(d.r) + '</span><div class="acts">' +
      '<button type="button" class="iconbtn sm" data-act="nome-edita" data-v="' + esc(d.r) + '" aria-label="Editar o nome de ' + esc(d.n) + '" title="Editar o nome">' + ic('edit') + '</button>' +
      '<button type="button" class="iconbtn sm go" data-act="call" data-v="' + esc(d.r) + '" aria-label="Ligar para ' + esc(d.n) + '" title="Ligar">' + ic('phone') + '</button><button type="button" class="iconbtn sm" data-act="msg" data-v="' + esc(d.r) + '" aria-label="Mensagem para ' + esc(d.n) + '" title="Mensagem">' + ic('chat') + '</button></div></div>'
  }
  const v = state.rq.trim()
  let html = lista.map(linha).join('')
  if (v && ehNumero(v) && !lista.some(d => d.r === v)) html = linha({ r: v, n: 'Ramal ' + v, conhecido: false, h: hue(v) }) + html
  el.innerHTML = html || (q ? '<div class="empty"><b>Ninguém com essa busca</b><span>Digite o número do ramal para ligar ou mandar mensagem.</span></div>' : '<div class="empty"><b>Ninguém por aqui ainda</b><span>Ligue ou mande uma mensagem pela aba Chamadas ou Conversas.</span></div>')
  const i = $('#nome-ed'); if (i) { i.focus(); i.select() }
}

// ---- Configuracoes ----
const sw = (key, on, label) => '<button type="button" class="sw" role="switch" aria-checked="' + !!on + '" aria-label="' + label + '" data-act="toggle" data-k="' + key + '"></button>'
const senhaOnde = c => c.senha === 'chaveiro' ? 'senha no chaveiro' : c.senha === 'arquivo' ? 'senha no arquivo' : 'sem senha'
function formHTML () {
  const f = state.form
  const nova = !f.idAntigo
  const campo = (k, label, extra, small, cls) => '<label class="campo' + (cls ? ' ' + cls : '') + '"><span>' + label + '</span><input data-f="' + k + '" value="' + esc(f[k] == null ? '' : f[k]) + '"' + (extra || '') + '>' + (small ? '<small>' + small + '</small>' : '') + '</label>'
  return '<form class="form" id="fconta" autocomplete="off"><h3>' + (nova ? 'Nova conta' : 'Editar ' + esc(f.idAntigo)) + '</h3><div class="campos">' +
    campo('usuario', 'Ramal', ' inputmode="numeric" spellcheck="false" placeholder="1001" required', 'O usuário SIP: em geral, o número do ramal.') +
    campo('nome', 'Nome que o outro lado vê', ' placeholder="Opcional"') +
    campo('dominio', 'Servidor do PBX', ' spellcheck="false" placeholder="voip.empresa.com.br" required') +
    campo('porta', 'Porta', ' inputmode="numeric" spellcheck="false"', 'UDP. A padrão do SIP é 5060.') +
    campo('senha', 'Senha', ' type="password"' + (nova ? ' required' : ' placeholder="Deixe vazio para manter a de agora"'), state.temChaveiro ? 'Fica no chaveiro do sistema.' : 'Esta máquina não tem chaveiro: fica num arquivo só seu.', 'largo') +
    '</div><div class="linha-sw">' + sw('form-registra', f.registra, 'Registrar a conta') + '<span>Registrar no PBX (para receber ligações e mensagens). Registrar tira o ramal de outro aparelho que esteja nele.</span></div>' +
    '<div class="linha-sw">' + sw('form-principal', f.principal, 'Conta principal') + '<span>Conta principal: as ligações e mensagens saem por ela. No teclado, a setinha do Ligar troca na hora.</span></div>' +
    (f.erro ? '<div class="erro" role="alert">' + esc(f.erro) + '</div>' : '') +
    '<div class="acoes"><button type="button" class="btn" data-act="form-cancela">Cancelar</button><button type="submit" class="btn prim"' + (f.salvando ? ' disabled' : '') + '>' + (f.salvando ? 'Salvando…' : 'Salvar') + '</button></div></form>'
}
const SECOES = [['contas', 'Contas', 'user'], ['audio', 'Áudio', 'mic'], ['toques', 'Toques e sons', 'bell'], ['aparencia', 'Aparência', 'sun'], ['janela', 'Janela', 'janela'], ['sobre', 'Sobre', 'info']]
// ocultaConfig = 1 no preferencias.toml (27/11, para instalar em massa): Contas, Toques e sons e Janela somem, e em
// Audio fica so o primeiro bloco. Quem administra muda o resto no arquivo (o Rust tambem recusa pela tela). Sem
// conta nenhuma, Contas fica: a primeira configuracao (o ramal e a senha) faz parte da instalacao.
const oculta = () => !!state.prefs.ocultaConfig
const primeiraConta = () => !state.contas.length
// Nao perturbe (27/11): recusa as ligacoes que chegam como ocupado. permitirNaoPerturbe = 0 no arquivo tira a opcao.
const podeNaoPerturbe = () => state.prefs.permitirNaoPerturbe !== false
const emNaoPerturbe = () => !!state.prefs.naoPerturbe && podeNaoPerturbe()
function statusHTML () {
  const dnd = emNaoPerturbe()
  const opcao = (v, nome, sub, cor, sim) => '<button type="button" class="opt" role="radio" aria-checked="' + sim + '" data-act="dnd" data-v="' + v + '"><span class="ck">' + ic('check') + '</span><span class="nm">' + nome + '<small class="bl">' + sub + '</small></span><span class="dot' + (v === '1' ? ' dnd' : '') + '" style="--c:' + cor + '"></span></button>'
  const contas = state.contas.map(c => {
    const [t, cor] = estadoConta(c), extra = [c.detalhe, pbxTexto(c)].filter(Boolean).join(' · ')
    const religa = ['falhou', 'parada', 'sem-senha'].includes(c.registro) || reconectando(c)
    return '<div class="pop-conta"><span class="dot" style="--c:' + cor + '"></span><div class="tx"><b>' + esc(c.usuario) + (c.nome ? ' · ' + esc(c.nome) : '') + '</b><small>' + t + (extra ? ' · ' + esc(extra) : '') + '</small></div>' +
      (religa ? '<button type="button" class="btn sm" data-act="reconecta" data-v="' + esc(c.id) + '">Reconectar</button>' : '') + '</div>'
  }).join('')
  return (podeNaoPerturbe()
    ? '<div class="pop-title">Status</div><div role="radiogroup" aria-label="Status">' + opcao('0', 'Disponível', 'Recebe as ligações.', 'var(--ok)', !dnd) +
      opcao('1', 'Não perturbe', 'Recusa as ligações como ocupado; elas ficam no histórico.', 'var(--crit)', dnd) + '</div><div class="pop-sep"></div>'
    : '<div class="pop-title">Conexão</div>') +
    contas + (oculta() ? '' : '<div class="pop-acoes"><button type="button" class="btn sm" data-act="pop-contas">' + ic('user') + 'Contas</button></div>')
}
const secoesVistas = () => oculta() ? SECOES.filter(s => ['audio', 'aparencia', 'sobre'].includes(s[0]) || (s[0] === 'contas' && primeiraConta())) : SECOES
const item = (b, small, vl) => '<div class="item"><div class="lb"><b>' + b + '</b>' + (small ? '<small>' + small + '</small>' : '') + '</div><div class="vl">' + vl + '</div></div>'
const grupo = (titulo, itens) => (titulo ? '<div class="grupo-titulo">' + titulo + '</div>' : '') + '<div class="grupo">' + itens + '</div>'
const cabeca = (titulo, texto) => '<header class="cfghead"><h2>' + titulo + '</h2>' + (texto ? '<p>' + texto + '</p>' : '') + '</header>'
const msegHTML = (act, atual, opcoes, rotulo) => '<div class="mseg" role="group" aria-label="' + rotulo + '">' + opcoes.map(([v, l]) => '<button type="button" data-act="' + act + '" data-v="' + v + '" aria-pressed="' + (atual === v) + '">' + l + '</button>').join('') + '</div>'
const contaComProblema = () => state.contas.some(c => c.registro === 'falhou' || c.registro === 'sem-senha')
function renderAjustes () {
  const vistas = secoesVistas()
  const sec = state.secao = vistas.some(s => s[0] === state.secao) ? state.secao : vistas[0][0]
  const secoes = { contas: secContas, audio: secAudio, toques: secToques, aparencia: secAparencia, janela: secJanela, sobre: secSobre }
  // Redesenhar a mesma secao (um botao la embaixo) nao volta para o topo.
  const antes = $('#cfgpane'), rolagem = antes && antes.dataset.sec === sec ? antes.scrollTop : 0
  $('#content').innerHTML = '<div class="cfg"><nav class="cfgnav" aria-label="Seções das configurações">' +
    vistas.map(([v, l, i]) => '<button type="button" data-act="secao" data-v="' + v + '"' + (sec === v ? ' aria-current="page"' : '') + '>' + ic(i) + '<span>' + l + '</span>' + (v === 'contas' && contaComProblema() ? '<span class="dot" style="--c:var(--warn)" title="Conta com problema"></span>' : '') + '</button>').join('') +
    '</nav><section class="cfgpane" id="cfgpane" data-sec="' + sec + '" aria-label="' + esc(vistas.find(x => x[0] === sec)[1]) + '">' + secoes[sec]() + '</section></div>'
  $('#cfgpane').scrollTop = rolagem
}
function secContas () {
  const linhas = state.contas.map(c => {
    const d = { r: c.usuario, n: c.nome || c.usuario, conhecido: !!c.nome, h: hue(c.usuario) }
    const linha = '<div class="conta">' + av(d) + '<div class="tx"><b>' + esc(c.usuario) + (c.nome ? ' · ' + esc(c.nome) : '') + (c.principal && state.contas.length > 1 ? ' <span class="pill principal">Principal</span>' : '') + '</b><small><span class="mono">' + esc(c.dominio) + (c.porta !== 5060 ? ':' + c.porta : '') + '</span>' + regPill(c) + '<span>' + senhaOnde(c) + '</span></small></div>' +
      '<div class="acts">' + (['falhou', 'parada'].includes(c.registro) ? '<button type="button" class="iconbtn sm" data-act="reconecta" data-v="' + esc(c.id) + '" aria-label="Reconectar ' + esc(c.id) + '" title="Reconectar">' + ic('refresh') + '</button>' : '') +
      '<button type="button" class="iconbtn sm" data-act="edita-conta" data-v="' + esc(c.id) + '" aria-label="Editar ' + esc(c.id) + '" title="Editar">' + ic('edit') + '</button>' +
      '<button type="button" class="iconbtn sm" data-act="remove-conta" data-v="' + esc(c.id) + '" aria-label="Remover ' + esc(c.id) + '" title="Remover">' + ic('trash') + '</button></div></div>'
    const confirma = state.remover === c.id
      ? '<div class="confirma" role="alert"><span>Remover a conta <b>' + esc(c.id) + '</b>? Ela sai do registro e a senha é apagada. O histórico fica.</span><button type="button" class="btn" data-act="remove-cancela">Cancelar</button><button type="button" class="btn perigo" data-act="remove-confirma" data-v="' + esc(c.id) + '">Remover</button></div>'
      : ''
    const form = state.form && state.form.idAntigo === c.id ? formHTML() : ''
    return linha + confirma + form
  }).join('')
  let html = cabeca('Contas', 'Os ramais deste computador. Alguns aparelhos usam mais de um.')
  if (state.contas.length) html += '<div class="grupo">' + linhas + '</div>'
  if (state.form && !state.form.idAntigo) html += formHTML()
  else if (!state.form) html += '<div class="acoes inicio"><button type="button" class="btn prim" data-act="nova-conta">' + ic('plus') + (state.contas.length ? 'Adicionar outra conta' : 'Cadastrar conta') + '</button></div>'
  return html
}
function secAudio () {
  const p = state.prefs, devs = state.devices
  const nomeDev = (lista, id) => { const d = id && (lista || []).find(x => x.id === id); return d ? d.nome : 'Padrão do sistema' }
  const pick = (k, titulo, lista) => '<button type="button" class="chip" data-act="pick" data-k="' + k + '" data-pop aria-haspopup="dialog" aria-expanded="false" title="' + titulo + '"><span>' + esc(nomeDev(lista, p[k])) + '</span>' + ic('caret', 'ic caret') + '</button>'
  const t = state.testeMic
  const teste = t
    ? '<div class="medidor" aria-hidden="true"><span id="medidor-barra"></span></div><span class="tab muted" id="medidor-db">…</span><button type="button" class="btn sm" data-act="para-mic">Parar</button>'
    : '<button type="button" class="btn sm" data-act="testa-mic">' + ic('mic') + 'Testar microfone</button>'
  const inicio = cabeca('Áudio', 'Por onde você fala e ouve nas ligações. "Padrão do sistema" segue o que está escolhido no computador.') +
    (state.devErro ? '<div class="erro" role="alert">' + esc(state.devErro) + '</div>' : '') +
    grupo('', item('Microfone', 'O que vai para o outro lado.', pick('microfone', 'Microfone', devs && devs.microfones)) +
      item('Testar o microfone', 'Fale alguma coisa: a barra mostra o que ele capta, por 10 segundos.', teste) +
      '<div id="teste-aviso"></div>' +
      item('Voz da ligação', 'Onde se ouve quem está do outro lado.', pick('altoFalante', 'Voz da ligação', devs && devs.altoFalantes)) +
      item('Pausar música e vídeo', 'Ao atender, pausa o que estiver tocando; ao desligar, volta a tocar só o que foi pausado.', sw('pausarMusica', p.pausarMusica !== false, 'Pausar música e vídeo durante a ligação')))
  if (state.erroPrefs) return inicio + '<div class="erro" role="alert">O arquivo de preferências tem um erro, e nada do que se muda aqui fica gravado até ele ser corrigido ou apagado:<pre class="mono">' + esc(state.erroPrefs) + '</pre></div>'
  if (oculta()) return inicio + '<p class="muted nota-admin">As outras opções deste computador são definidas pelo administrador.</p>'
  return inicio +
    grupo('Qualidade da voz',
      item('Cancelar o eco', 'Com caixa de som em vez de headset, o outro lado não ouve a própria voz de volta.', sw('cancelarEco', p.cancelarEco !== false, 'Cancelar o eco')) +
      item('Reduzir o ruído do microfone', 'Tira o ruído constante daqui (ventilador, ar-condicionado) do que vai para o outro lado.', sw('reduzirRuido', p.reduzirRuido !== false, 'Reduzir o ruído do microfone')) +
      item('Voz mais nítida', 'Limpa o ruído do que chega do outro lado. Vale a partir da próxima ligação, como as outras.', sw('vozNitida', p.vozNitida !== false, 'Voz mais nítida'))) +
    secGravacao()
}
function secGravacao () {
  const p = state.prefs, g = state.grav, r = state.retencao
  const dias = p.guardarGravacoes == null ? 90 : p.guardarGravacoes
  const resumo = !g ? '…' : g.quantidade ? plural(g.quantidade, 'gravação', 'gravações') + ' · ' + tamanho(g.bytes) : 'Nenhuma gravação ainda.'
  // Diminuir a retencao apaga na hora o que passou dela: so depois do "Apagar".
  const confirma = r
    ? '<div class="confirma" role="alert"><span>Guardar por ' + rotuloDias(r.dias) + ' apaga agora <b>' + plural(r.n, 'gravação', 'gravações') + '</b> com mais tempo que isso.</span>' +
      '<button type="button" class="btn" data-act="retencao-cancela">Cancelar</button><button type="button" class="btn perigo" data-act="retencao-confirma">Apagar</button></div>'
    : ''
  return grupo('Gravação',
    item('Gravar as ligações', 'As duas vozes de cada ligação atendida, em Opus: uns 5 MB por hora de conversa.', sw('gravar', p.gravar, 'Gravar as ligações')) +
    item('Guardar por', 'As mais antigas se apagam sozinhas.', msegHTML('retencao', String(r ? r.dias : dias), [['7', '7 dias'], ['30', '30 dias'], ['90', '90 dias'], ['365', '1 ano'], ['0', 'Sempre']], 'Guardar as gravações por')) + confirma +
    item('Pasta', (g ? '<span class="mono">' + esc(g.pasta) + '</span><br>' : '') + resumo, '<button type="button" class="btn sm" data-act="abre-gravacoes">Abrir</button>'))
}
async function carregaGravacoes () {
  try { state.grav = await invoke('gravacoes') } catch (e) { state.grav = null; return }
  if (state.view === 'ajustes' && (state.secao === 'audio' || state.secao === 'sobre')) renderView()
}
// O tocador fica fora do que se redesenha (o historico), para nao recomecar a cada aviso.
async function ouveGravacao (arquivo) {
  if (state.call) { toast('Durante a ligação, não.'); return }
  let bytes
  try { bytes = await invoke('ouve_gravacao', { arquivo }) } catch (e) { toast('Não abri a gravação: ' + erroDe(e)); return }
  fechaGravacao()
  const h = state.hist.find(x => x.gravacao === arquivo)
  const d = h ? pessoa(h.ramal) : null
  // No Windows os bytes chegam num ArrayBuffer; no Linux o IPC do WebKit (postMessage) manda uma lista de numeros.
  const dados = bytes instanceof ArrayBuffer ? bytes : Uint8Array.from(bytes)
  const url = URL.createObjectURL(new Blob([dados], { type: 'audio/ogg; codecs=opus' }))
  state.ouvindo = { arquivo, url }
  const el = $('#ouvindo')
  el.innerHTML = ic('play') + '<div class="tx"><b>' + (d ? esc(d.n) : 'Gravação') + '</b>' + (h ? '<span>' + (h.entrada ? 'Recebida' : 'Feita') + ' · ' + esc(dia(h.quando)) + ', ' + hora(h.quando) + '</span>' : '') + '</div>' +
    '<audio controls autoplay src="' + url + '"></audio>' +
    '<button type="button" class="iconbtn sm ghost" data-act="ouvir-fecha" aria-label="Fechar a gravação" title="Fechar">' + ic('x') + '</button>'
  el.hidden = false
}
function fechaGravacao () {
  const el = $('#ouvindo'), a = el.querySelector('audio')
  if (a) a.pause()
  el.hidden = true; el.innerHTML = ''
  if (state.ouvindo) URL.revokeObjectURL(state.ouvindo.url)
  state.ouvindo = null
}
function secToques () {
  const p = state.prefs, vol = p.volumeToque == null ? 50 : p.volumeToque
  const toq = t => '<div class="toq" role="radio" tabindex="0" aria-checked="' + ((p.toque || 'colmeia') === t.id) + '" data-act="toque" data-v="' + esc(t.id) + '"><span class="ck">' + ic('check') + '</span><span class="nm">' + esc(t.nome) + '</span>' +
    '<button type="button" class="iconbtn sm ghost" data-act="ouvir-toque" data-v="' + esc(t.id) + '" aria-label="Ouvir ' + esc(t.nome) + '" title="Ouvir">' + ic('play') + '</button></div>'
  // Os fortes (27/11) ficam separados: para o callcenter, a sala com barulho.
  const lista = (titulo, forte) => '<div class="grupo-titulo">' + titulo + '</div><div class="toques" role="radiogroup" aria-label="' + titulo + '">' + state.toques.filter(t => !!t.forte === forte).map(toq).join('') + '</div>'
  return cabeca('Toques e sons', 'O toque de quem liga e os sons da janela.') +
    lista('Toques discretos', false) + lista('Toques fortes, para sala com barulho', true) +
    grupo('Como toca',
      item('Volume do toque', p.toqueNoMaximo ? 'Com o volume máximo ligado, o toque vai cheio.' : '', '<input id="vol-toque" type="range" min="0" max="100" step="5" value="' + vol + '" aria-label="Volume do toque"' + (p.toqueNoMaximo ? ' disabled' : '') + '><span class="tab muted" id="vol-toque-v">' + vol + '%</span>') +
      item('Tocar em todos os alto-falantes', 'Headset e caixas de som ao mesmo tempo, para ouvir de longe.', sw('toqueEmTodos', p.toqueEmTodos, 'Tocar em todos os alto-falantes')) +
      item('Volume máximo enquanto toca', 'Sobe a saída a 100% e tira o mudo enquanto toca. Quando para, volta ao volume de antes.', sw('toqueNoMaximo', p.toqueNoMaximo, 'Volume máximo enquanto toca'))) +
    grupo('Sons da janela',
      item('Estilo', 'Chamando, teclas, fim da ligação e mensagem.', msegHTML('sons', p.sons || 'modernos', [['modernos', 'Modernos'], ['classicos', 'Clássicos']], 'Estilo dos sons') +
        '<button type="button" class="iconbtn sm" data-act="ouvir" data-v="chamando" aria-label="Ouvir o chamando" title="Ouvir o chamando">' + ic('play') + '</button>'))
}
function secAparencia () {
  const p = state.prefs
  return cabeca('Aparência', '') +
    grupo('', item('Tema', 'Sistema acompanha o claro ou escuro do computador.', msegHTML('tema', p.tema || 'sistema', [['sistema', 'Sistema'], ['claro', 'Claro'], ['escuro', 'Escuro']], 'Tema')) +
      item('Visual', 'Muda no arquivo de preferências: <span class="mono">flavor = "mcalls"</span> liga o M Calls.', '<span class="pill" style="--c:var(--primary)"><span class="dot"></span>' + brandName() + '</span>'))
}
function secJanela () {
  const p = state.prefs
  return cabeca('Janela', 'Fechar a janela leva para a bandeja: o app continua recebendo ligação.') +
    grupo('', item('Trazer para a frente', 'Quando chega ligação, a janela aparece na frente das outras. O aviso no canto da tela aparece sempre.', sw('trazerParaFrente', p.trazerParaFrente, 'Trazer a janela para a frente')) +
      item('Abrir com o sistema', 'Abre só na bandeja ao entrar no computador, pronto para receber ligação.', state.autostart == null ? '<span class="muted">…</span>' : sw('autostart', state.autostart, 'Abrir com o sistema')))
}
function secSobre () {
  // O caminho vai embaixo, em linha inteira: nao espreme o texto (27/11).
  const caminho = (b, small, valor) => '<div class="item col"><div class="lb"><b>' + b + '</b><small>' + small + '</small></div><div class="mono caminho">' + esc(valor) + '</div></div>'
  return cabeca(brandName(), 'Softphone SIP para o computador, com ligações e mensagens.') +
    grupo('', item('Feito por', '', '<span>Jurandir Moratelli</span><span class="muted">@JJMoratelli</span>') +
      item('Versão', state.atualizaSozinho ? 'As versões novas chegam sozinhas: o app avisa e instala quando você mandar.' : '', '<span class="tab">' + esc(state.versao) + '</span>' +
        (state.atualizaSozinho ? '<button type="button" class="btn sm" data-act="procura-atualizacao"' + (state.procurando ? ' disabled' : '') + '>' + ic('refresh') + (state.procurando ? 'Procurando…' : 'Procurar atualização') + '</button>' : '')) +
      item('Licença', 'Código aberto: github.com/JMoratelli/HexagonCall', '<span>GNU GPL 3</span>') +
      caminho('Arquivos', 'Contas, preferências, histórico e conversas, em texto. A senha fica no chaveiro do sistema.', state.pasta) +
      (state.grav ? caminho('Gravações', 'As ligações gravadas (Ajustes → Áudio).', state.grav.pasta) : ''))
}
/** O teste do microfone: atualiza a barra sem redesenhar a tela (o evento vem a cada 100 ms). */
function mostraNivel (a) {
  const t = state.testeMic; if (!t) return
  if (a) Object.assign(t, { db: a.db, mudo: a.mudo, erro: a.erro })
  if (a && a.fim) { state.testeMic = null; if (a.erro) toast('O teste não abriu o microfone: ' + a.erro); if (state.view === 'ajustes' && state.secao === 'audio') renderView(); return }
  const barra = $('#medidor-barra'), db = $('#medidor-db'), aviso = $('#teste-aviso')
  if (barra) { barra.style.width = Math.max(0, Math.min(100, (t.db + 60) / 60 * 100)) + '%'; barra.dataset.forte = t.db > -40 ? '1' : '0' }
  if (db) db.textContent = t.db <= -119 ? 'silêncio' : Math.round(t.db) + ' dB'
  if (aviso) aviso.innerHTML = t.mudo
    ? '<div class="alerta" role="status">' + ic('micoff') + '<div><b>Este microfone está mudo no sistema</b><span>Ou com o volume em zero.</span></div><button type="button" class="btn sm prim" data-act="tira-do-mudo">Tirar do mudo</button></div>'
    : ''
}
async function carregaDispositivos () {
  try { state.devices = await invoke('dispositivos'); state.devErro = null } catch (e) { state.devErro = erroDe(e) }
  if (state.view === 'ajustes') renderView()
}
async function salvaPrefs () {
  try { await invoke('salva_preferencias', { p: state.prefs }) } catch (e) { toast('Não salvei: ' + erroDe(e)) }
}
function novoForm (c) {
  state.remover = null
  state.form = c
    ? { idAntigo: c.id, usuario: c.usuario, nome: c.nome || '', dominio: c.dominio, porta: String(c.porta), registra: c.registra, principal: !!c.principal, senha: '', erro: null, salvando: false }
    : { idAntigo: null, usuario: '', nome: '', dominio: '', porta: '5060', registra: true, principal: !state.contas.length, senha: '', erro: null, salvando: false }
}
async function salvaForm () {
  const f = state.form; if (!f || f.salvando) return
  const porta = Number(f.porta)
  if (!f.usuario.trim() || !f.dominio.trim()) { f.erro = 'Preencha o ramal e o servidor.'; return renderView() }
  if (!Number.isInteger(porta) || porta < 1 || porta > 65535) { f.erro = 'A porta vai de 1 a 65535.'; return renderView() }
  f.salvando = true; f.erro = null; renderView()
  try {
    const e = await invoke('salva_conta', { conta: { idAntigo: f.idAntigo, usuario: f.usuario.trim(), dominio: f.dominio.trim(), porta, nome: f.nome.trim() || null, registra: f.registra, principal: f.principal, senha: f.senha || null } })
    aplica(e); state.form = null; toast('Conta salva. Conectando ao PBX…')
    // Com ocultaConfig, Contas some depois da primeira: segue para o teclado.
    if (oculta()) { go('chamadas'); return }
  } catch (e) {
    f.salvando = false; f.erro = erroDe(e)
  }
  refresh()
}

// ---- Ligacao ----
async function startCall (r) {
  r = r == null ? '' : String(r).trim()
  if (!r) return toast('Digite um ramal ou número.')
  if (state.call) return toast('Já tem uma ligação. Desligue antes de ligar de novo.')
  if (!state.contas.length) return toast('Cadastre uma conta antes de ligar.')
  closePop()
  state.call = { id: null, kind: 'outgoing', r, tocando: false }
  state.panel = 'call'; state.dial = ''
  if (state.view === 'chamadas') renderLeft(); renderStrip()
  try {
    const conta = state.contaLigacao && state.contas.some(c => c.id === state.contaLigacao) ? state.contaLigacao : null
    state.contaLigacao = null
    state.call.id = await invoke('liga', { destino: r, conta })
  } catch (e) {
    state.call = null; refresh(); toast('Não liguei: ' + erroDe(e))
  }
}
const daChamada = id => state.call && state.call.id === id
function mandaTom (k) {
  const c = state.call; if (!c || c.kind !== 'active') return
  state.dtmf = (state.dtmf + k).slice(-24)
  const d = $('#dtmf-disp'); if (d) d.value = state.dtmf
  invoke('tons', { id: c.id, teclas: k }).catch(e => toast('O tom não saiu: ' + erroDe(e)))
}
async function acao (cmd, args, falha) {
  try { await invoke(cmd, args); return true } catch (e) { toast(falha + ': ' + erroDe(e)); return false }
}

// ---- Sons ----
// Os da janela, sintetizados aqui (WebAudio), sem arquivo de som. O toque de quem liga nao e daqui: o telefone toca
// (som::toque), para sair no alto-falante escolhido ou em todos. "Modernos": o chamando sao dois acordes suaves;
// "Clássicos": o tom de chamada brasileiro (425 Hz, 1 s ligado e 4 s desligado). As teclas tocam o par do DTMF, baixo.
let audioCtx = null
function ctxSom () {
  try {
    audioCtx = audioCtx || new AudioContext()
    if (audioCtx.state === 'suspended') audioCtx.resume().catch(() => {})
    return audioCtx
  } catch (e) { return null }
}
/** Uma nota (ou acorde): sobe em 15 ms e morre devagar; `plano` segura o volume ate o fim, como o tom classico. */
function nota (freqs, quando, dur, vol, plano) {
  const c = ctxSom(); if (!c) return
  const t = c.currentTime + quando, g = c.createGain()
  g.gain.setValueAtTime(0, t)
  g.gain.linearRampToValueAtTime(vol, t + 0.015)
  if (plano) { g.gain.setValueAtTime(vol, t + dur - 0.02); g.gain.linearRampToValueAtTime(0, t + dur) } else g.gain.exponentialRampToValueAtTime(0.0001, t + dur)
  g.connect(c.destination)
  for (const f of [].concat(freqs)) { const o = c.createOscillator(); o.frequency.value = f; o.connect(g); o.start(t); o.stop(t + dur + 0.05) }
}
let repeticao = null
function repete (tocar, cada) { paraSom(); tocar(); repeticao = setInterval(tocar, cada) }
function paraSom () { if (repeticao) { clearInterval(repeticao); repeticao = null } }
const DTMF = { 1: [697, 1209], 2: [697, 1336], 3: [697, 1477], 4: [770, 1209], 5: [770, 1336], 6: [770, 1477], 7: [852, 1209], 8: [852, 1336], 9: [852, 1477], '*': [941, 1209], 0: [941, 1336], '#': [941, 1477] }
const SONS = {
  chamando: () => state.prefs.sons === 'classicos'
    ? repete(() => nota(425, 0, 1, 0.045, true), 5000)
    : repete(() => { nota([523.25, 659.25], 0, 0.7, 0.03); nota([587.33, 783.99], 0.75, 0.8, 0.026) }, 3200),
  tecla: k => { if (DTMF[k]) nota(DTMF[k], 0, state.prefs.sons === 'classicos' ? 0.12 : 0.09, state.prefs.sons === 'classicos' ? 0.03 : 0.02, state.prefs.sons === 'classicos') },
  fim: () => state.prefs.sons === 'classicos'
    ? [0, 0.35, 0.7].forEach(t => nota(425, t, 0.25, 0.04, true))
    : (nota(659.25, 0, 0.18, 0.04), nota(440, 0.16, 0.32, 0.04)),
  mensagem: () => { nota(987.77, 0, 0.14, 0.035); nota(1318.5, 0.08, 0.22, 0.03) }
}

// ---- Avisos do telefone ----
function aplica (e) {
  state.contas = e.contas; state.prefs = e.preferencias; state.erroPrefs = e.erroPreferencias; state.temChaveiro = e.temChaveiro; state.pasta = e.pasta
  state.hist = e.historico; state.convs = e.conversas; state.toques = e.toques || []
  state.apelidos = e.apelidos || {}; state.presencas = e.presencas || {}
  if (e.chamada && !state.call) {
    const c = e.chamada
    state.call = { id: c.id, kind: c.atendida ? 'active' : c.entrada ? 'incoming' : 'outgoing', r: c.ramal, t0: c.desde * 1000, gravando: c.gravando }
    if (c.nome) state.nomes[c.ramal] = c.nome
  }
}
function recebe (a) {
  switch (a.tipo) {
    case 'presenca':
      state.presencas[a.ramal] = a.estado
      if (state.view === 'ramais' && !state.editaNome) renderDlist()
      break
    case 'abre-conversa': state.open = a.ramal; go('conversas'); { const t = $('#msg-in'); if (t) t.focus() } break
    case 'pbx': {
      const c = state.contas.find(x => x.id === a.conta); if (!c) break
      const mudou = c.semResposta !== a.semResposta || (c.pbxMs == null) !== (a.pbxMs == null)
      Object.assign(c, { pbxMs: a.pbxMs, pbxEm: a.pbxEm, semResposta: a.semResposta })
      if (a.semResposta && !c.foraDesde) c.foraDesde = Math.round(Date.now() / 1000)
      renderChrome()
      if (mudou && state.view === 'ajustes' && (state.secao || 'contas') === 'contas' && !state.form) renderView()
      break
    }
    case 'contas':
      state.contas = a.contas; renderChrome()
      if (state.view === 'ajustes' && !(state.form && document.activeElement && document.activeElement.closest('#fconta'))) renderView()
      if (state.view === 'chamadas' && !$('#lpanel') && state.contas.length) renderView()
      break
    case 'recebendo':
      if (a.nome) state.nomes[a.ramal] = a.nome
      { const ouvindo = $('#ouvindo audio'); if (ouvindo) ouvindo.pause() }
      state.call = { id: a.id, kind: 'incoming', r: a.ramal, conta: a.conta }
      state.panel = 'call' // o toque, o telefone toca (no alto-falante escolhido ou em todos)
      if (state.view === 'chamadas') renderLeft(); renderStrip()
      break
    case 'tocando':
      if (daChamada(a.id) && !state.call.tocando) { state.call.tocando = true; SONS.chamando(); if (state.view === 'chamadas') renderLeft(); renderStrip() }
      break
    case 'atendida':
      if (daChamada(a.id) || (state.call && !state.call.id && state.call.kind === 'outgoing')) {
        paraSom()
        Object.assign(state.call, { id: a.id, kind: 'active', t0: Date.now(), codec: a.codec, gravando: a.gravando })
        if (state.view === 'chamadas') renderLeft(); renderStrip()
      }
      break
    case 'espera':
      if (daChamada(a.id)) { state.call.deles = a.sim; if (state.view === 'chamadas') renderLeft(); renderStrip() }
      break
    case 'nivel': mostraNivel(a); break
    case 'microfone':
      if (daChamada(a.id)) {
        state.call.semSom = a.silencio
        state.call.micMudo = a.mudo
        state.call.alternativas = a.alternativas
        if (a.silencio && !state.devices) carregaDispositivos()
        if (state.view === 'chamadas') renderLeft(); renderStrip()
      }
      break
    case 'sem-audio':
      if (daChamada(a.id)) {
        state.call.semMic = a.microfone
        state.call.semFone = a.altoFalante
        if (a.microfone && !state.devices) carregaDispositivos()
        if (state.view === 'chamadas') renderLeft(); renderStrip()
      }
      break
    case 'erro': toast(a.texto); break
    case 'encerrada': {
      state.hist.push(a.item)
      if (daChamada(a.id)) { const era = state.call.kind; paraSom(); state.call = null; if (era !== 'incoming') SONS.fim() }
      const d = pessoa(a.item.ramal)
      if (perdida(a.item) && state.view !== 'chamadas') state.missedNew++
      toast(peloNaoPerturbe(a.item) ? 'Ligação de ' + d.n + ' recusada: não perturbe.' : perdida(a.item) ? 'Ligação perdida de ' + d.n + '.' : emOutroRamal(a.item) ? 'Ligação de ' + d.n + ' atendida em outro ramal.' : transferida(a.item) ? 'Ligação transferida · ' + fmt(a.item.duracao) : a.item.atendida ? 'Ligação encerrada · ' + fmt(a.item.duracao) : a.item.fim)
      refresh()
      break
    }
    case 'mensagem': {
      const m = a.item
      state.convs.push(m)
      const vendo = state.view === 'conversas' && state.open === m.ramal
      if (!vendo) state.naoLidas[m.ramal] = (state.naoLidas[m.ramal] || 0) + 1
      renderChrome()
      if (state.view === 'conversas') { if (vendo) { const b = $('#tbody'); if (b) { b.innerHTML = threadHTML(m.ramal); scrollThread() } } renderClist() }
      if (!vendo) { toast(pessoa(m.ramal).n + ': ' + m.texto); if (!state.call) SONS.mensagem() }
      break
    }
  }
}

// ---- Atualizacao ----
const UM_DIA = 24 * 3600 * 1000
function recebeAtualizacao (a) {
  switch (a.tipo) {
    case 'disponivel': {
      const d = state.atuDepois
      const mesma = state.atu && state.atu.rel.tag === a.release.tag
      if (mesma && state.atu.fase !== 'disponivel') break // ja baixando ou instalando
      if (d && d.tag === a.release.tag && Date.now() - d.quando < UM_DIA) break
      state.atu = { rel: a.release, fase: 'disponivel', fracao: 0 }
      break
    }
    case 'baixando': if (state.atu) Object.assign(state.atu, { fase: 'baixando', fracao: a.fracao }); break
    case 'instalando': if (state.atu) state.atu.fase = 'instalando'; break
    case 'erro': if (state.atu) Object.assign(state.atu, { fase: 'erro', erro: a.texto }); break
  }
  renderAtualiza()
}
function renderAtualiza () {
  const el = $('#atualiza'), a = state.atu
  el.hidden = !a
  if (!a) { el.innerHTML = ''; return }
  const v = esc(a.rel.tag.replace(/^v/i, ''))
  let tx, ctl = ''
  if (a.fase === 'baixando') {
    const pct = Math.round((a.fracao || 0) * 100)
    tx = '<b>Baixando a versão ' + v + '…</b><span class="tab">' + pct + '%</span>'
    ctl = '<div class="medidor atu-barra" role="progressbar" aria-label="Download da atualização" aria-valuemin="0" aria-valuemax="100" aria-valuenow="' + pct + '"><span style="width:' + pct + '%" data-forte="1"></span></div>'
  } else if (a.fase === 'instalando') {
    tx = '<b>Instalando a versão ' + v + '…</b><span>O app fecha e abre de novo sozinho.</span>'
  } else if (a.fase === 'erro') {
    // Os erros do Rust vem em minuscula e sem ponto ("não baixei a atualização: ..."); alguns ja vem com ponto.
    let erro = a.erro || ''
    erro = erro.charAt(0).toUpperCase() + erro.slice(1)
    if (erro && !/[.!?]$/.test(erro)) erro += '.'
    tx = '<b>A atualização não foi instalada</b><span>' + esc(erro) + '</span>'
    ctl = '<button type="button" class="btn sm" data-act="atualiza-depois">Depois</button><button type="button" class="btn sm prim" data-act="atualiza-agora">Tentar de novo</button>'
  } else {
    tx = '<b>Versão ' + v + ' disponível</b><span>Você tem a ' + esc(state.versao) + '. Para atualizar, o app fecha e abre de novo.</span>'
    ctl = '<button type="button" class="btn sm" data-act="atualiza-depois">Depois</button><button type="button" class="btn sm prim" data-act="atualiza-agora">' + ic('baixar') + 'Atualizar agora</button>'
  }
  el.innerHTML = ic('baixar') + '<div class="tx">' + tx + '</div>' + ctl
}
async function atualizaAgora () {
  if (state.call) { toast('Termine a ligação antes de atualizar.'); return }
  Object.assign(state.atu, { fase: 'baixando', fracao: 0, erro: null }); renderAtualiza()
  try {
    // false: a lista lida de novo ja nao tem versao mais nova (a faixa era velha).
    if (await invoke('atualiza') === false) { state.atu = null; renderAtualiza(); toast('Você já está na versão mais nova.') }
  } catch (e) { if (state.atu) { Object.assign(state.atu, { fase: 'erro', erro: erroDe(e) }); renderAtualiza() } }
}
async function procuraAtualizacao () {
  state.procurando = true; if (state.view === 'ajustes') renderView()
  try {
    const r = await invoke('procura_atualizacao')
    if (r) { state.atuDepois = null; recebeAtualizacao({ tipo: 'disponivel', release: r }) } else toast('Você já está na versão mais nova.')
  } catch (e) { toast('Não consegui procurar: ' + erroDe(e)) }
  state.procurando = false; if (state.view === 'ajustes') renderView()
}

// ---- Popover e aviso ----
let popAnchor = null, popKey = null
function openPop (anchor, html, key) {
  const p = $('#pop')
  if (popKey === key && !p.hidden) return closePop()
  closePop()
  p.innerHTML = html; p.hidden = false; popKey = key; popAnchor = anchor
  anchor.setAttribute('aria-expanded', 'true')
  const a = anchor.getBoundingClientRect(), w = p.offsetWidth, h = p.offsetHeight
  const left = Math.min(Math.max(12, a.left), innerWidth - w - 12)
  let top = a.bottom + 6
  if (top + h > innerHeight - 12) top = Math.max(12, a.top - h - 6)
  p.style.left = left + 'px'; p.style.top = top + 'px'
}
function closePop () {
  const p = $('#pop'); p.hidden = true; popKey = null
  if (popAnchor) popAnchor.setAttribute('aria-expanded', 'false')
  popAnchor = null
}
let toastT, volT
function toast (msg) { const t = $('#toast'); t.textContent = msg; t.hidden = false; clearTimeout(toastT); toastT = setTimeout(() => { t.hidden = true }, 3800) }

// ---- Telas ----
function renderView () { ({ chamadas: renderChamadas, conversas: renderConversas, ramais: renderRamais, ajustes: renderAjustes })[state.view]() }
function refresh () { renderChrome(); renderStrip(); renderView() }
function go (v) {
  if (state.testeMic && v !== 'ajustes') invoke('para_teste_microfone').catch(() => {})
  state.view = VIEWS.includes(v) ? v : 'chamadas'
  state.sel = null; state.editaNome = null
  closePop()
  if (state.view === 'ajustes' && !state.devices) carregaDispositivos()
  if (state.view === 'ajustes') carregaGravacoes()
  if (state.view === 'ajustes' && state.autostart == null) invoke('abrir_com_o_sistema', { sim: null }).then(v => { state.autostart = v; if (state.view === 'ajustes') renderView() }).catch(() => {})
  refresh()
}

// ---- Eventos da tela ----
const DIALABLE = /^[0-9*#+]$/
document.addEventListener('click', e => {
  if (!$('#pop').hidden && !e.target.closest('#pop') && !e.target.closest('[data-pop]')) closePop()
}, true)
document.addEventListener('click', async e => {
  const b = e.target.closest('[data-act]'); if (!b) return
  const act = b.dataset.act, v = b.dataset.v
  const c = state.call
  switch (act) {
    case 'go': go(v); break
    case 'gear': go(state.view === 'ajustes' ? 'chamadas' : 'ajustes'); break
    case 'hseg': state.hseg = v; renderRecent(); focusDial(); break
    case 'fill':
      if (c) { toast('Termine a ligação para discar outro número.'); break }
      state.dial = v; { const i = $('#dial-main'); if (i) i.value = v } renderMatch(); focusDial(); break
    case 'open': state.open = v; state.cq = ''; renderView(); renderChrome(); { const t = $('#msg-in'); if (t && finePointer()) t.focus() } break
    case 'msg': state.open = v; go('conversas'); break
    case 'call': startCall(v); break
    case 'dialkey':
      SONS.tecla(v)
      if (b.dataset.m === 'dtmf') { mandaTom(v); break }
      if (b.dataset.m === 'transfer') { state.dialT = (state.dialT + v).slice(0, 30); const i = $('#dial-transfer'); if (i) i.value = state.dialT; focusDial(); break }
      state.dial = (state.dial + v).slice(0, 30); { const i = $('#dial-main'); if (i) i.value = state.dial } renderMatch(); focusDial(); break
    case 'dialback':
      if (b.dataset.m === 'transfer') { state.dialT = state.dialT.slice(0, -1); const i = $('#dial-transfer'); if (i) i.value = state.dialT; focusDial(); break }
      state.dial = state.dial.slice(0, -1); { const i = $('#dial-main'); if (i) i.value = state.dial } renderMatch(); focusDial(); break
    case 'panel': state.panel = v; if (v === 'dtmf') state.dtmf = ''; if (v === 'transfer') state.dialT = ''; renderLeft(); break
    case 'transfere': {
      const t = state.dialT.trim()
      if (!c || !t) { toast('Digite o ramal para onde transferir.'); break }
      if (!ehNumero(t)) { toast('Digite o número do ramal.'); break }
      if (await acao('transfere', { id: c.id, destino: t }, 'Não transferi')) { toast('Transferindo para ' + t + '…'); state.panel = 'call'; renderLeft() }
      break
    }
    // Um clique no achado da busca ja liga (pedido de 27/11).
    case 'dialpick': startCall(v); break
    case 'dialgo': {
      const t = dialTarget()
      if (!t) { toast(state.dial.trim() ? 'Ninguém com esse nome por aqui. Digite o ramal.' : 'Digite um ramal ou número.'); break }
      startCall(t); break
    }
    case 'answer':
      if (c && c.kind === 'incoming') await acao('atende', { id: c.id }, 'Não atendi')
      break
    case 'decline': if (c) await acao('desliga', { id: c.id }, 'Não recusei'); break
    case 'hangup':
      if (!c) break
      if (!c.id) { toast('Ainda conectando, tente de novo em um instante.'); break }
      await acao('desliga', { id: c.id }, 'Não desliguei'); break
    case 'mute':
      if (c && c.kind === 'active' && await acao('mudo', { id: c.id, sim: !c.muted }, 'Não mudei o mudo')) { c.muted = !c.muted; if (state.view === 'chamadas') renderLeft(); renderStrip() }
      break
    case 'hold':
      if (c && c.kind === 'active' && await acao('espera', { id: c.id, sim: !c.held }, 'A espera não funcionou')) { c.held = !c.held; renderLeft(); renderStrip() }
      break
    case 'nova-conta': if (oculta() && !primeiraConta()) break; novoForm(null); state.secao = 'contas'; if (state.view !== 'ajustes') go('ajustes'); else renderView(); { const i = document.querySelector('#fconta input'); if (i) i.focus() } break
    case 'edita-conta': if (oculta()) break; novoForm(state.contas.find(x => x.id === v)); renderView(); break
    case 'form-cancela': state.form = null; renderView(); break
    case 'remove-conta': if (oculta()) break; state.form = null; state.remover = v; renderView(); break
    case 'remove-cancela': state.remover = null; renderView(); break
    case 'tira-do-mudo':
      if (await acao('tira_do_mudo', {}, 'Não tirei do mudo')) { toast('Microfone fora do mudo. Fale alguma coisa.'); if (state.call) { state.call.micMudo = false; if (state.view === 'chamadas') renderLeft() } if (state.testeMic) { state.testeMic.mudo = false; mostraNivel(null) } }
      break
    case 'usa-microfone':
      try { await invoke('troca_microfone', { microfone: v }); state.prefs.microfone = v; toast('Microfone trocado. Fale alguma coisa.') } catch (err) { toast('Não troquei: ' + erroDe(err)) }
      break
    case 'testa-mic':
      if (state.call) { toast('Durante a ligação, não.'); break }
      state.testeMic = { db: -120, mudo: false, erro: null }
      renderView()
      try { await invoke('testa_microfone', { microfone: state.prefs.microfone || null }) } catch (err) { state.testeMic = null; toast('Não testei: ' + erroDe(err)); renderView() }
      break
    case 'para-mic': invoke('para_teste_microfone').catch(() => {}); break
    case 'reconecta': b.disabled = true; if (b.closest('#pop')) closePop(); if (await acao('reconecta', { id: v }, 'Não reconectei')) toast('Conectando de novo ao PBX…'); break
    case 'remove-confirma':
      try { aplica(await invoke('remove_conta', { id: v })); toast('Conta removida.') } catch (err) { toast('Não removi: ' + erroDe(err)) }
      state.remover = null; refresh(); break
    case 'toggle': {
      const k = b.dataset.k
      if (k === 'form-registra') { state.form.registra = !state.form.registra; b.setAttribute('aria-checked', state.form.registra); break }
      if (k === 'form-principal') { state.form.principal = !state.form.principal; b.setAttribute('aria-checked', state.form.principal); break }
      if (k === 'autostart') {
        try { state.autostart = await invoke('abrir_com_o_sistema', { sim: !state.autostart }) } catch (err) { toast('Não mudei: ' + erroDe(err)) }
        b.setAttribute('aria-checked', !!state.autostart); break
      }
      state.prefs[k] = !state.prefs[k]; b.setAttribute('aria-checked', !!state.prefs[k]); salvaPrefs()
      if (k === 'toqueNoMaximo') renderView()
      break
    }
    case 'tema': state.prefs.tema = v; salvaPrefs(); refresh(); break
    case 'atualiza-agora': await atualizaAgora(); break
    case 'atualiza-depois': if (state.atu) state.atuDepois = { tag: state.atu.rel.tag, quando: Date.now() }; state.atu = null; renderAtualiza(); break
    case 'procura-atualizacao': await procuraAtualizacao(); break
    case 'secao': state.secao = v; state.retencao = null; closePop(); renderView(); { const pane = $('#cfgpane'); if (pane) pane.scrollTop = 0 } if (v === 'audio' || v === 'sobre') carregaGravacoes(); break
    case 'retencao': {
      const dias = Number(v), atual = state.prefs.guardarGravacoes == null ? 90 : state.prefs.guardarGravacoes
      if (dias === atual && !state.retencao) break
      state.retencao = null
      // Qualquer retencao que apague gravacao agora pede o "Apagar" antes de valer.
      let n = 0
      if (dias > 0) { try { n = await invoke('gravacoes_mais_velhas', { dias }) } catch (err) { toast('Não conferi as gravações: ' + erroDe(err)); renderView(); break } }
      if (n > 0) { state.retencao = { dias, n }; renderView(); break }
      state.prefs.guardarGravacoes = dias; await salvaPrefs(); renderView(); break
    }
    case 'retencao-cancela': state.retencao = null; renderView(); break
    case 'retencao-confirma': {
      const r = state.retencao; if (!r) break
      state.retencao = null
      state.prefs.guardarGravacoes = r.dias
      await salvaPrefs()
      try { const n = await invoke('aplica_retencao'); toast(n ? plural(n, 'gravação apagada', 'gravações apagadas') + '.' : 'Nenhuma gravação apagada.') } catch (err) { toast('Não apaguei: ' + erroDe(err)) }
      await carregaGravacoes(); renderView(); break
    }
    case 'abre-gravacoes': try { await invoke('abre_pasta_gravacoes') } catch (err) { toast('Não abri a pasta: ' + erroDe(err)) } break
    case 'ouvir-gravacao': await ouveGravacao(v); break
    case 'sel-entra': state.sel = { marcadas: new Set(), confirma: false, gravacoes: true }; renderRecent(); break
    case 'sel-sai': state.sel = null; renderRecent(); break
    case 'marca': if (state.sel) { const m = state.sel.marcadas; if (m.has(v)) m.delete(v); else m.add(v); state.sel.confirma = false; renderRecent() } break
    case 'marca-todas': {
      const s = state.sel; if (!s) break
      const visiveis = [...state.hist].sort((x, y) => y.quando - x.quando).filter(h => state.hseg === 'all' || perdida(h)).slice(0, 200).map(chaveHist)
      if (s.marcadas.size === visiveis.length) s.marcadas.clear(); else visiveis.forEach(k => s.marcadas.add(k))
      s.confirma = false; renderRecent(); break
    }
    case 'exclui-hist': if (state.sel && state.sel.marcadas.size) { state.sel.confirma = true; renderRecent() } break
    case 'exclui-nao': if (state.sel) { state.sel.confirma = false; renderRecent() } break
    case 'exclui-sim': {
      const s = state.sel; if (!s || !s.marcadas.size) break
      // A escolha da caixa fica no state (um redesenho no meio nao a desfaz).
      const gravacoes = !!$('#exclui-gravacoes') && s.gravacoes
      const itens = state.hist.filter(h => s.marcadas.has(chaveHist(h))).map(h => ({ quando: h.quando, conta: h.conta, ramal: h.ramal, entrada: h.entrada, atendida: h.atendida, duracao: h.duracao, gravacao: h.gravacao || null }))
      try {
        const r = await invoke('exclui_historico', { itens, gravacoes })
        state.hist = state.hist.filter(h => !s.marcadas.has(chaveHist(h)))
        state.sel = null
        toast(plural(r.ligacoes, 'ligação excluída', 'ligações excluídas') + (r.gravacoes ? ' e ' + plural(r.gravacoes, 'gravação apagada', 'gravações apagadas') : '') + '.')
      } catch (err) { toast('Não excluí: ' + erroDe(err)) }
      renderRecent(); break
    }
    case 'nome-edita': state.editaNome = v; renderDlist(); break
    case 'nome-nao': state.editaNome = null; renderDlist(); break
    case 'nome-sim': {
      const i = $('#nome-ed'), nome = i ? i.value : ''
      try { state.apelidos = await invoke('salva_nome', { ramal: v, nome }); state.editaNome = null; toast(nome.trim() ? 'Nome salvo.' : 'Nome tirado.') } catch (err) { toast('Não salvei: ' + erroDe(err)) }
      renderDlist(); break
    }
    case 'ouvir-fecha': fechaGravacao(); break
    case 'ouvir-toque':
      if (state.call) { toast('Durante a ligação, não.'); break }
      b.disabled = true
      try { await invoke('ouve_toque', { toque: v }) } catch (err) { toast('Não tocou: ' + erroDe(err)) }
      b.disabled = false; break
    case 'sons': state.prefs.sons = v; salvaPrefs(); renderView(); break
    case 'ouvir':
      if (state.call) { toast('Durante a ligação, não.'); break }
      SONS[v](); setTimeout(paraSom, state.prefs.sons === 'classicos' ? 1200 : 1600); break
    case 'toque': state.prefs.toque = v; salvaPrefs(); renderView(); break
    case 'minimizar': janela.minimize(); break
    case 'fechar': janela.close(); break
    case 'status': openPop(b, statusHTML(), 'status'); break
    case 'dnd': {
      closePop()
      try { aplica(await invoke('nao_perturbe', { sim: v === '1' })); refresh(); toast(v === '1' ? 'Não perturbe: as ligações serão recusadas.' : 'Disponível: as ligações voltam a tocar.') } catch (err) { toast('Não mudei: ' + erroDe(err)) }
      break
    }
    case 'pop-contas': closePop(); state.secao = 'contas'; go('ajustes'); break
    case 'pick-conta': {
      const atual = contaDaLigacao()
      openPop(b, '<div class="pop-title">Ligar por</div><div role="radiogroup">' + state.contas.map(c => {
        const [t, cor] = REGISTRO[c.registro] || REGISTRO.parada
        return '<button type="button" class="opt" role="radio" aria-checked="' + (atual && atual.id === c.id) + '" data-act="conta-ligacao" data-v="' + esc(c.id) + '"><span class="ck">' + ic('check') + '</span><span class="nm">' + esc(c.usuario) + (c.nome ? ' <small>· ' + esc(c.nome) + '</small>' : '') + (c.principal ? ' <small>· principal</small>' : '') + '</span><span class="dot" style="--c:' + cor + '" title="' + t + '"></span></button>'
      }).join('') + '</div>', 'pick-conta')
      break
    }
    case 'conta-ligacao': {
      const c = state.contas.find(x => x.id === v)
      state.contaLigacao = c && !c.principal ? v : null
      closePop(); renderLeft(); break
    }
    case 'pick': {
      const k = b.dataset.k
      const lista = state.devices ? (k === 'altoFalante' ? state.devices.altoFalantes : state.devices.microfones) : []
      const atual = state.prefs[k === 'microfoneLigacao' ? 'microfone' : k]
      const opcoes = lista.length ? lista : [{ id: null, nome: 'Padrão do sistema' }]
      openPop(b, '<div class="pop-title">' + esc(b.title) + '</div><div role="radiogroup">' +
        opcoes.map(o => '<button type="button" class="opt" role="radio" aria-checked="' + ((!o.id && !atual) || o.id === atual) + '" data-act="picked" data-k="' + k + '" data-v="' + esc(o.id || '') + '"><span class="ck">' + ic('check') + '</span><span class="nm">' + esc(o.nome) + '</span></button>').join('') + '</div>', 'pick-' + k)
      break
    }
    case 'picked': {
      const k = b.dataset.k, id = v || null
      closePop()
      if (k === 'microfoneLigacao') {
        try { await invoke('troca_microfone', { microfone: id }); state.prefs.microfone = id; toast('Microfone trocado. Fale alguma coisa.') } catch (err) { toast('Não troquei: ' + erroDe(err)) }
        break
      }
      state.prefs[k] = id; salvaPrefs(); renderView(); break
    }
  }
})
document.addEventListener('input', e => {
  const t = e.target
  if (t.id === 'cq') { state.cq = t.value; renderClist() }
  else if (t.id === 'rq') { state.rq = t.value; renderDlist() }
  else if (t.id === 'exclui-gravacoes' && state.sel) state.sel.gravacoes = t.checked
  else if (t.id === 'msg-in') { state.drafts[state.open] = t.value; $('#sendbtn').disabled = !t.value.trim(); autosize() }
  else if (t.id === 'dial-main') { state.dial = t.value.slice(0, 30); renderMatch() }
  else if (t.id === 'dial-transfer') state.dialT = t.value.slice(0, 30)
  else if (t.id === 'vol-toque') { state.prefs.volumeToque = +t.value; $('#vol-toque-v').textContent = t.value + '%'; clearTimeout(volT); volT = setTimeout(salvaPrefs, 400) }
  else if (t.dataset.f && state.form) state.form[t.dataset.f] = t.value
})
// So a interface do app: sem o menu do navegador (Inspecionar, Recarregar, Imprimir) e sem os atalhos dele
// (recarregar, imprimir, buscar, salvar, voltar, o inspetor). Nos campos de texto o menu fica: copiar e colar.
// No Windows nem nos campos: o menu do WebView2 traz o "Importar Senhas" do Edge, que abre uma janela solta com erro e
// nao sai sem unsafe. Copiar e colar seguem pelo teclado (Ctrl+C, Ctrl+V).
const semMenuNosCampos = /Windows/.test(navigator.userAgent)
document.addEventListener('contextmenu', e => { if (semMenuNosCampos || !e.target.closest('input, textarea')) e.preventDefault() })
document.addEventListener('keydown', e => {
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key, ctrl = e.ctrlKey || e.metaKey
  const doNavegador = ['F1', 'F3', 'F5', 'F7', 'F11', 'F12', 'BrowserBack', 'BrowserForward', 'BrowserRefresh'].includes(e.key) ||
    (ctrl && ['r', 'p', 'f', 'g', 's', 'u', 'j', 'o', 'h', 'd', '+', '=', '-', '0'].includes(k)) ||
    (ctrl && e.shiftKey && ['i', 'j', 'c', 'r'].includes(k)) ||
    (e.altKey && ['ArrowLeft', 'ArrowRight', 'Home'].includes(e.key))
  if (doNavegador) { e.preventDefault(); e.stopPropagation() }
}, true)
document.addEventListener('keydown', e => {
  const t = e.target
  if ((e.key === 'Enter' || e.key === ' ') && t.classList && t.classList.contains('toq')) { e.preventDefault(); t.click(); return }
  if (e.key === 'Escape' && !$('#pop').hidden) { const a = popAnchor; closePop(); if (a) a.focus(); return }
  if (t.id === 'msg-in' && e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(); return }
  if (t.id === 'nome-ed' && (e.key === 'Enter' || e.key === 'Escape')) { e.preventDefault(); const b = $(e.key === 'Enter' ? '[data-act="nome-sim"]' : '[data-act="nome-nao"]'); if (b) b.click(); return }
  if (t.id === 'rq' && e.key === 'Enter') { const g = $('#dlist [data-act="call"]'); if (g) g.click(); return }
  if (t.id === 'cq' && e.key === 'Enter') { const v = t.value.trim(); const primeiroItem = $('#clist .citem'); if (primeiroItem) primeiroItem.click(); else if (v) { state.open = v; state.cq = ''; renderView() } return }
  if (t.id === 'dial-main' && e.key === 'Enter') { e.preventDefault(); const g = document.querySelector('[data-act="dialgo"]'); if (g) g.click(); return }
  if (t.id === 'dial-transfer' && e.key === 'Enter') { e.preventDefault(); const g = document.querySelector('[data-act="transfere"]'); if (g) g.click(); return }
  if (e.ctrlKey || e.metaKey || e.altKey || state.view !== 'chamadas') return
  // Com o teclado aberto na ligacao, digitar manda o tom
  if (state.call && state.call.kind === 'active' && state.panel === 'dtmf' && DIALABLE.test(e.key)) { e.preventDefault(); SONS.tecla(e.key); mandaTom(e.key); return }
  if (state.call) return
  if (t.closest && t.closest('input, textarea, .pop')) return
  // Na aba Chamadas, digitar com o foco fora do campo cai no campo
  if (e.key.length === 1 && e.key !== ' ' && $('#dial-main')) {
    e.preventDefault(); state.dial = (state.dial + e.key).slice(0, 30)
    const i = $('#dial-main'); i.value = state.dial; i.focus({ preventScroll: true })
    renderMatch()
  }
})
document.addEventListener('submit', e => {
  e.preventDefault()
  if (e.target.id === 'cform') send()
  else if (e.target.id === 'fconta') salvaForm()
})
window.addEventListener('resize', () => { if (!$('#pop').hidden) closePop() })
setInterval(() => {
  const c = state.call
  if (!c || c.kind !== 'active') return
  const s = fmt((Date.now() - c.t0) / 1000)
  const a = $('#cp-timer'), b = $('#strip-timer')
  if (a) a.textContent = s
  if (b) b.textContent = s
}, 1000)

async function abre () {
  await listen('telefone', ev => recebe(ev.payload))
  await listen('atualizacao', ev => recebeAtualizacao(ev.payload))
  try { aplica(await invoke('estado')) } catch (e) { toast('Não li o estado: ' + erroDe(e)) }
  try {
    const a = await invoke('atualizacao')
    state.versao = a.versao; state.atualizaSozinho = a.suportado
    if (a.pendente) recebeAtualizacao({ tipo: 'disponivel', release: a.pendente })
  } catch (e) { state.versao = '' }
  if (!state.contas.length) { go('ajustes'); novoForm(null); renderView() } else go('chamadas')
}
abre()
