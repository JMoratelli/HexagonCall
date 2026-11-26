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
  min: '<path d="M6 12h12"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>'
}
const ic = (n, cls) => '<svg class="' + (cls || 'ic') + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + ICONS[n] + '</svg>'
const MARK = '<svg viewBox="0 0 28 28" aria-hidden="true"><path d="M8 3.5h12l6 10.5-6 10.5H8L2 14z" fill="var(--art-a)"/><path d="M10.5 9h7l3.5 5-3.5 5h-7L7 14z" fill="var(--art-b)"/></svg>'

const VIEWS = ['chamadas', 'conversas', 'ramais', 'ajustes']
const state = {
  view: 'chamadas', contas: [], prefs: { marca: 'mcalls', microfone: null, altoFalante: null, trazerParaFrente: true },
  temChaveiro: true, pasta: '', hist: [], convs: [], nomes: {}, toques: [],
  call: null, panel: 'call', dial: '', dialT: '', dtmf: '', hseg: 'all', missedNew: 0,
  open: null, cq: '', drafts: {}, naoLidas: {},
  form: null, remover: null, devices: null, devErro: null, autostart: null
}

// ---- Pequenos pedacos ----
const fmt = s => { s = Math.max(0, s | 0); const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), x = String(s % 60).padStart(2, '0'); return h ? h + ':' + String(m).padStart(2, '0') + ':' + x : m + ':' + x }
const hora = t => new Date(t * 1000).toTimeString().slice(0, 5)
function dia (t) {
  const d = new Date(t * 1000), hoje = new Date()
  const zero = x => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  const dias = Math.round((zero(hoje) - zero(d)) / 86400000)
  if (dias === 0) return 'Hoje'
  if (dias === 1) return 'Ontem'
  return String(d.getDate()).padStart(2, '0') + '/' + String(d.getMonth() + 1).padStart(2, '0')
}
const finePointer = () => matchMedia('(pointer: fine)').matches
const brandName = () => state.prefs.marca === 'hexagon' ? 'Hexagon Call' : 'M Calls'
function hue (r) { let h = 0; for (const c of String(r)) h = (h * 31 + c.charCodeAt(0)) % 360; return h }
const ini = n => { const w = n.split(/\s+/).filter(x => /\p{L}/u.test(x) && !/^(da|de|do|dos|das)$/i.test(x)); return (w.length > 1 ? w[0][0] + w[w.length - 1][0] : (w[0] || n).slice(0, 2)).toUpperCase() }

/** O nome de um ramal: o que veio na ligacao ou na mensagem (o PBX manda no From). */
function nomeDe (r) {
  if (state.nomes[r]) return state.nomes[r]
  for (let i = state.hist.length - 1; i >= 0; i--) if (state.hist[i].ramal === r && state.hist[i].nome) return state.hist[i].nome
  for (let i = state.convs.length - 1; i >= 0; i--) if (state.convs[i].ramal === r && state.convs[i].nome) return state.convs[i].nome
  return null
}
const pessoa = r => { const n = nomeDe(r); return { r, n: n || r, conhecido: !!n, h: hue(r) } }
const primeiro = d => d.conhecido ? d.n.split(' ')[0] : d.r
const av = (d, size) => '<span class="av' + (size ? ' ' + size : '') + '" style="--h:' + d.h + '" aria-hidden="true">' + esc(d.conhecido ? ini(d.n) : '#') + '</span>'
const ehNumero = v => /^[0-9*#+]+$/.test(v)
const contaPrincipal = () => state.contas.find(c => c.registro === 'registrado') || state.contas[0] || null
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
const regPill = c => { const [t, cor] = REGISTRO[c.registro] || REGISTRO.parada; return '<span class="pill" style="--c:' + cor + '"' + (c.detalhe ? ' title="' + esc(c.detalhe) + '"' : '') + '><span class="dot"></span>' + t + (c.detalhe ? ' · ' + esc(c.detalhe) : '') + '</span>' }

// ---- A barra do app: marca, abas, registro e engrenagem ----
function renderChrome () {
  document.documentElement.dataset.scheme = state.prefs.marca === 'hexagon' ? 'green' : 'red'
  document.title = brandName()
  const unread = Object.values(state.naoLidas).reduce((a, b) => a + b, 0)
  const tabs = [['chamadas', 'Chamadas', state.missedNew, 'var(--crit)'], ['conversas', 'Conversas', unread, 'var(--primary)'], ['ramais', 'Ramais']]
  const c = contaPrincipal()
  let status
  if (!c) status = '<button type="button" class="status" style="--c:var(--warn)" data-act="nova-conta" title="Cadastre o seu ramal"><span class="dot"></span>Sem conta</button>'
  else {
    const [t, cor] = REGISTRO[c.registro] || REGISTRO.parada
    status = '<button type="button" class="status" style="--c:' + cor + '" data-act="go" data-v="ajustes" title="' + esc(c.id + ' · ' + t + (c.detalhe ? ': ' + c.detalhe : '')) + '"><span class="dot"></span>' + esc(c.usuario) + (state.contas.length > 1 ? ' +' + (state.contas.length - 1) : '') + '</button>'
  }
  $('#abar').innerHTML = '<span class="brand" data-tauri-drag-region>' + MARK + '<span>' + brandName() + '</span></span>' +
    '<nav class="segs" aria-label="Abas">' + tabs.map(([v, label, n, tone]) => '<button type="button" class="seg" data-act="go" data-v="' + v + '"' + (state.view === v ? ' aria-current="page"' : '') + '>' + label + (n ? '<span class="n" style="--c:' + tone + '">' + n + '</span>' : '') + '</button>').join('') + '</nav>' +
    status +
    '<button type="button" class="iconbtn" data-act="gear" aria-label="Configurações" title="Configurações" aria-pressed="' + (state.view === 'ajustes') + '">' + ic('gear') + '</button>' +
    '<span class="wbtns"><button type="button" class="iconbtn ghost sm" data-act="minimizar" aria-label="Minimizar" title="Minimizar">' + ic('min') + '</button>' +
    '<button type="button" class="iconbtn ghost sm fecha" data-act="fechar" aria-label="Fechar (continua na bandeja)" title="Fechar (continua na bandeja)">' + ic('x') + '</button></span>'
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
    '<div class="match" id="match-main">' + matchHTML() + '</div>' + padHTML('main') +
    '<button type="button" class="callbtn" data-act="dialgo">' + ic('phone') + 'Ligar</button>'
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
  return list.map((d, i) => '<button type="button" class="opt" data-act="dialpick" data-v="' + esc(d.r) + '"' + (i === 0 ? ' data-first="1"' : '') + ' tabindex="-1">' + av(d, 'sm') + '<span class="nm">' + esc(d.n) + '</span><span class="ct">' + esc(d.r) + '</span>' + (i === 0 && !ehNumero(v) ? '<kbd>Enter</kbd>' : '') + '</button>').join('')
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
  const notas = (c.held ? ' · <b>em espera</b>' : '') + (c.deles ? ' · <b>' + esc(primeiro(d)) + ' pôs em espera</b>' : '') + (c.muted ? ' · <b>mudo</b>' : '')
  const alerta = c.semSom
    ? '<div class="alerta" role="status">' + ic('micoff') + '<div><b>Não te ouço</b><span>O microfone está mandando silêncio. Quer testar outro?</span></div><button type="button" class="btn sm" data-act="pick" data-k="microfoneLigacao" data-pop aria-haspopup="dialog" aria-expanded="false" title="Microfone">Trocar</button></div>'
    : ''
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
  return '<div class="cp ringing"><div class="cp-who">' + av(d, 'xl') + '<b>' + esc(d.n) + '</b><small>' + esc(c.r) + '</small></div><span class="cp-state">' + ic('pin') + 'Chamada recebida</span></div>' +
    '<div class="two"><button type="button" class="hangbtn" data-act="decline">' + ic('hang') + 'Recusar</button><button type="button" class="answerbtn" data-act="answer">' + ic('phone') + 'Atender</button></div>'
}
const perdida = h => h.entrada && !h.atendida
function histTipo (h) {
  if (perdida(h)) return h.fim === 'Ligação recusada' ? 'Recusada' : 'Perdida'
  if (!h.atendida) return 'Não completada'
  return h.entrada ? 'Recebida' : 'Feita'
}
const histIcone = h => perdida(h) ? ['pmiss', 'var(--crit)'] : h.entrada ? ['pin', 'var(--ok)'] : ['pout', 'var(--info)']
function renderRecent () {
  const el = $('#recent'); if (!el) return
  const todas = [...state.hist].reverse()
  const missed = todas.filter(perdida).length
  const rows = todas.filter(h => state.hseg === 'all' || perdida(h))
  let grupo = '', body = ''
  for (const h of rows.slice(0, 200)) {
    const g = dia(h.quando)
    if (g !== grupo) { grupo = g; body += '<div class="rgrp">' + esc(g) + '</div>' }
    const d = pessoa(h.ramal), [icon, c] = histIcone(h)
    const sub = esc(h.ramal) + ' · <span class="k">' + histTipo(h) + '</span>' + (h.duracao ? ' · ' + fmt(h.duracao) : '') + (!h.atendida && !perdida(h) ? ' · ' + esc(h.fim) : '')
    body += '<div class="hrow"><button type="button" class="hmain' + (perdida(h) ? ' missed' : '') + '" data-act="fill" data-v="' + esc(h.ramal) + '" title="Pôr ' + esc(h.ramal) + ' no campo">' +
      '<span class="dir" style="--c:' + c + '">' + ic(icon) + '</span><span class="tx"><b>' + esc(d.n) + '</b><small>' + sub + '</small></span><span class="when">' + hora(h.quando) + '</span></button>' +
      '<button type="button" class="iconbtn sm go" data-act="call" data-v="' + esc(h.ramal) + '" aria-label="Ligar para ' + esc(d.n) + '" title="Ligar">' + ic('phone') + '</button>' +
      '<button type="button" class="iconbtn sm" data-act="msg" data-v="' + esc(h.ramal) + '" aria-label="Mensagem para ' + esc(d.n) + '" title="Mensagem">' + ic('chat') + '</button></div>'
  }
  const vazio = state.hseg === 'all'
    ? '<div class="empty"><b>Nenhuma ligação ainda</b><span>As ligações feitas e recebidas neste computador aparecem aqui.</span></div>'
    : '<div class="empty"><b>Nenhuma perdida</b><span>Quando alguém ligar e você não atender, aparece aqui.</span></div>'
  el.innerHTML = '<header class="rhead"><h2>Últimas chamadas</h2><div class="mseg" role="group" aria-label="Filtro">' +
    '<button type="button" data-act="hseg" data-v="all" aria-pressed="' + (state.hseg === 'all') + '">Todas</button>' +
    '<button type="button" data-act="hseg" data-v="missed" aria-pressed="' + (state.hseg === 'missed') + '">Perdidas <span class="n">' + missed + '</span></button></div></header>' +
    '<div class="rlist">' + (body || vazio) + '</div>'
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
    const item = await invoke('mensagem', { destino: r, texto, conta: null })
    state.convs.push(item); state.drafts[r] = ''
    if (state.view === 'conversas' && state.open === r) { renderThread(); renderClist(); const t = $('#msg-in'); if (t) t.focus() }
  } catch (e) {
    toast('A mensagem não saiu: ' + erroDe(e)); $('#sendbtn').disabled = false
  }
}

// ---- Ramais ----
function renderRamais () {
  const lista = conhecidos()
  $('#content').innerHTML = '<div class="tools"><span class="muted">A lista completa de ramais, com o nome cadastrado no PBX, vem numa próxima versão. Por enquanto aparece aqui quem já ligou ou mandou mensagem.</span></div>' +
    '<div class="dlist" id="dlist">' + (lista.length ? lista.map(d =>
      '<div class="drow">' + av(d) + '<div class="tx"><b>' + esc(d.n) + '</b><small>' + (d.conhecido ? esc(d.r) : 'sem nome ainda') + '</small></div>' +
      '<span class="rnum">' + esc(d.r) + '</span><div class="acts"><button type="button" class="iconbtn sm go" data-act="call" data-v="' + esc(d.r) + '" aria-label="Ligar para ' + esc(d.n) + '" title="Ligar">' + ic('phone') + '</button><button type="button" class="iconbtn sm" data-act="msg" data-v="' + esc(d.r) + '" aria-label="Mensagem para ' + esc(d.n) + '" title="Mensagem">' + ic('chat') + '</button></div></div>'
    ).join('') : '<div class="empty"><b>Ninguém por aqui ainda</b><span>Ligue ou mande uma mensagem pela aba Chamadas ou Conversas.</span></div>') + '</div>'
}

// ---- Configuracoes ----
const crow = (b, small, vl) => '<div class="crow"><div class="lb"><b>' + b + '</b>' + (small ? '<small>' + small + '</small>' : '') + '</div><div class="vl">' + vl + '</div></div>'
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
    (f.erro ? '<div class="erro" role="alert">' + esc(f.erro) + '</div>' : '') +
    '<div class="acoes"><button type="button" class="btn" data-act="form-cancela">Cancelar</button><button type="submit" class="btn prim"' + (f.salvando ? ' disabled' : '') + '>' + (f.salvando ? 'Salvando…' : 'Salvar') + '</button></div></form>'
}
function renderAjustes () {
  const p = state.prefs
  let contas = state.contas.map(c => {
    const d = { r: c.usuario, n: c.nome || c.usuario, conhecido: !!c.nome, h: hue(c.usuario) }
    const linha = '<div class="conta">' + av(d) + '<div class="tx"><b>' + esc(c.usuario) + (c.nome ? ' · ' + esc(c.nome) : '') + '</b><small><span class="mono">' + esc(c.dominio) + (c.porta !== 5060 ? ':' + c.porta : '') + '</span>' + regPill(c) + '<span>' + senhaOnde(c) + '</span></small></div>' +
      '<div class="acts"><button type="button" class="iconbtn sm" data-act="edita-conta" data-v="' + esc(c.id) + '" aria-label="Editar ' + esc(c.id) + '" title="Editar">' + ic('edit') + '</button>' +
      '<button type="button" class="iconbtn sm" data-act="remove-conta" data-v="' + esc(c.id) + '" aria-label="Remover ' + esc(c.id) + '" title="Remover">' + ic('trash') + '</button></div></div>'
    const confirma = state.remover === c.id
      ? '<div class="confirma" role="alert"><span>Remover a conta <b>' + esc(c.id) + '</b>? Ela sai do registro e a senha é apagada. O histórico fica.</span><button type="button" class="btn" data-act="remove-cancela">Cancelar</button><button type="button" class="btn perigo" data-act="remove-confirma" data-v="' + esc(c.id) + '">Remover</button></div>'
      : ''
    const form = state.form && state.form.idAntigo === c.id ? formHTML() : ''
    return linha + confirma + form
  }).join('')
  if (state.form && !state.form.idAntigo) contas += formHTML()
  else if (!state.form) contas += '<div class="crow"><div class="lb"><b>' + (state.contas.length ? 'Outra conta' : 'Nenhuma conta') + '</b><small>Alguns aparelhos usam mais de um ramal.</small></div><div class="vl"><button type="button" class="btn" data-act="nova-conta">' + ic('plus') + 'Adicionar conta</button></div></div>'
  const devs = state.devices
  const nomeDev = (lista, id) => { const d = id && (lista || []).find(x => x.id === id); return d ? d.nome : 'Padrão do sistema' }
  const pick = (k, titulo, lista) => '<button type="button" class="chip" data-act="pick" data-k="' + k + '" data-pop aria-haspopup="dialog" aria-expanded="false" title="' + titulo + '"><span>' + esc(nomeDev(lista, p[k])) + '</span>' + ic('caret', 'ic caret') + '</button>'
  $('#content').innerHTML = '<div class="cfgwrap">' +
    '<div class="csec">Contas</div>' + contas +
    '<div class="csec">Áudio</div>' +
    (state.devErro ? crow('Dispositivos', '', '<span class="erro">' + esc(state.devErro) + '</span>') : '') +
    crow('Microfone', '', pick('microfone', 'Microfone', devs && devs.microfones)) +
    crow('Voz da ligação', 'Onde se ouve quem está do outro lado.', pick('altoFalante', 'Voz da ligação', devs && devs.altoFalantes)) +
    '<div class="csec">Toque e sons</div>' +
    crow('Toque de quem liga', '', '<div class="mseg" role="group" aria-label="Toque de quem liga">' + state.toques.map(t => '<button type="button" data-act="toque" data-v="' + esc(t.id) + '" aria-pressed="' + ((p.toque || 'colmeia') === t.id) + '">' + esc(t.nome) + '</button>').join('') + '</div>' +
      '<button type="button" class="btn sm" data-act="ouvir" data-v="toque">Ouvir</button>') +
    crow('Volume do toque', p.toqueEmTodos ? 'Em todos os alto-falantes, o toque vai no máximo.' : '', '<input id="vol-toque" type="range" min="0" max="100" step="5" value="' + (p.volumeToque == null ? 50 : p.volumeToque) + '" aria-label="Volume do toque"' + (p.toqueEmTodos ? ' disabled' : '') + '><span class="tab muted" id="vol-toque-v">' + (p.volumeToque == null ? 50 : p.volumeToque) + '%</span>') +
    crow('Tocar em todos os alto-falantes', 'No volume máximo, para ouvir de longe. Quando para de tocar, cada saída volta ao volume de antes.', sw('toqueEmTodos', p.toqueEmTodos, 'Tocar em todos os alto-falantes')) +
    crow('Sons da janela', 'Chamando, teclas, fim da ligação e mensagem.', '<div class="mseg" role="group" aria-label="Sons da janela">' + [['modernos', 'Modernos'], ['classicos', 'Clássicos']].map(([v, l]) => '<button type="button" data-act="sons" data-v="' + v + '" aria-pressed="' + ((p.sons || 'modernos') === v) + '">' + l + '</button>').join('') + '</div>' +
      '<button type="button" class="btn sm" data-act="ouvir" data-v="chamando">Ouvir o chamando</button>') +
    '<div class="csec">Janela</div>' +
    crow('Trazer para a frente', 'Quando chega ligação, a janela aparece na frente das outras.', sw('trazerParaFrente', p.trazerParaFrente, 'Trazer a janela para a frente')) +
    crow('Abrir com o sistema', 'Abre só na bandeja ao entrar no computador, pronto para receber ligação.', state.autostart == null ? '<span class="muted">…</span>' : sw('autostart', state.autostart, 'Abrir com o sistema')) +
    crow('Cores', '', '<div class="mseg" role="group" aria-label="Cores">' + [['mcalls', 'M Calls'], ['hexagon', 'Hexagon Call']].map(([v, l]) => '<button type="button" data-act="marca" data-v="' + v + '" aria-pressed="' + (p.marca === v) + '">' + l + '</button>').join('') + '</div>') +
    '<div class="csec">Sobre</div>' +
    crow(brandName(), '', '<span class="tab">0.1.0</span><span class="muted">GNU GPL · SIP e áudio em Rust</span>') +
    crow('Arquivos', 'Contas, preferências, histórico e conversas, em texto.', '<span class="mono">' + esc(state.pasta) + '</span>') +
    '</div>'
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
    ? { idAntigo: c.id, usuario: c.usuario, nome: c.nome || '', dominio: c.dominio, porta: String(c.porta), registra: c.registra, senha: '', erro: null, salvando: false }
    : { idAntigo: null, usuario: '', nome: '', dominio: '', porta: '5060', registra: true, senha: '', erro: null, salvando: false }
}
async function salvaForm () {
  const f = state.form; if (!f || f.salvando) return
  const porta = Number(f.porta)
  if (!f.usuario.trim() || !f.dominio.trim()) { f.erro = 'Preencha o ramal e o servidor.'; return renderView() }
  if (!Number.isInteger(porta) || porta < 1 || porta > 65535) { f.erro = 'A porta vai de 1 a 65535.'; return renderView() }
  f.salvando = true; f.erro = null; renderView()
  try {
    const e = await invoke('salva_conta', { conta: { idAntigo: f.idAntigo, usuario: f.usuario.trim(), dominio: f.dominio.trim(), porta, nome: f.nome.trim() || null, registra: f.registra, senha: f.senha || null } })
    aplica(e); state.form = null; toast('Conta salva. Conectando ao PBX…')
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
    state.call.id = await invoke('liga', { destino: r, conta: null })
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
  state.contas = e.contas; state.prefs = e.preferencias; state.temChaveiro = e.temChaveiro; state.pasta = e.pasta
  state.hist = e.historico; state.convs = e.conversas; state.toques = e.toques || []
  if (e.chamada && !state.call) {
    const c = e.chamada
    state.call = { id: c.id, kind: c.atendida ? 'active' : c.entrada ? 'incoming' : 'outgoing', r: c.ramal, t0: c.desde * 1000 }
    if (c.nome) state.nomes[c.ramal] = c.nome
  }
}
function recebe (a) {
  switch (a.tipo) {
    case 'contas':
      state.contas = a.contas; renderChrome()
      if (state.view === 'ajustes' && !(state.form && document.activeElement && document.activeElement.closest('#fconta'))) renderView()
      if (state.view === 'chamadas' && !$('#lpanel') && state.contas.length) renderView()
      break
    case 'recebendo':
      if (a.nome) state.nomes[a.ramal] = a.nome
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
        Object.assign(state.call, { id: a.id, kind: 'active', t0: Date.now(), codec: a.codec })
        if (state.view === 'chamadas') renderLeft(); renderStrip()
      }
      break
    case 'espera':
      if (daChamada(a.id)) { state.call.deles = a.sim; if (state.view === 'chamadas') renderLeft(); renderStrip() }
      break
    case 'microfone':
      if (daChamada(a.id)) {
        state.call.semSom = a.silencio
        if (a.silencio && !state.devices) carregaDispositivos()
        if (state.view === 'chamadas') renderLeft(); renderStrip()
      }
      break
    case 'encerrada': {
      state.hist.push(a.item)
      if (daChamada(a.id)) { const era = state.call.kind; paraSom(); state.call = null; if (era !== 'incoming') SONS.fim() }
      const d = pessoa(a.item.ramal)
      if (perdida(a.item) && state.view !== 'chamadas') state.missedNew++
      toast(perdida(a.item) ? 'Ligação perdida de ' + d.n + '.' : a.item.atendida ? 'Ligação encerrada · ' + fmt(a.item.duracao) : a.item.fim)
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
  state.view = VIEWS.includes(v) ? v : 'chamadas'
  closePop()
  if (state.view === 'ajustes' && !state.devices) carregaDispositivos()
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
    case 'dialpick': state.dial = v; { const i = $('#dial-main'); if (i) i.value = v } renderMatch(); focusDial(); break
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
    case 'nova-conta': novoForm(null); if (state.view !== 'ajustes') go('ajustes'); else renderView(); { const i = document.querySelector('#fconta input'); if (i) i.focus() } break
    case 'edita-conta': novoForm(state.contas.find(x => x.id === v)); renderView(); break
    case 'form-cancela': state.form = null; renderView(); break
    case 'remove-conta': state.form = null; state.remover = v; renderView(); break
    case 'remove-cancela': state.remover = null; renderView(); break
    case 'remove-confirma':
      try { aplica(await invoke('remove_conta', { id: v })); toast('Conta removida.') } catch (err) { toast('Não removi: ' + erroDe(err)) }
      state.remover = null; refresh(); break
    case 'toggle': {
      const k = b.dataset.k
      if (k === 'form-registra') { state.form.registra = !state.form.registra; b.setAttribute('aria-checked', state.form.registra); break }
      if (k === 'autostart') {
        try { state.autostart = await invoke('abrir_com_o_sistema', { sim: !state.autostart }) } catch (err) { toast('Não mudei: ' + erroDe(err)) }
        b.setAttribute('aria-checked', !!state.autostart); break
      }
      state.prefs[k] = !state.prefs[k]; b.setAttribute('aria-checked', !!state.prefs[k]); salvaPrefs()
      if (k === 'toqueEmTodos') renderView()
      break
    }
    case 'marca': state.prefs.marca = v; salvaPrefs(); refresh(); break
    case 'sons': state.prefs.sons = v; salvaPrefs(); renderView(); break
    case 'ouvir':
      if (state.call) { toast('Durante a ligação, não.'); break }
      if (v === 'toque') { b.disabled = true; try { await invoke('ouve_toque', { toque: state.prefs.toque || 'colmeia' }) } catch (err) { toast('Não tocou: ' + erroDe(err)) } b.disabled = false; break }
      SONS[v](); setTimeout(paraSom, state.prefs.sons === 'classicos' ? 1200 : 1600); break
    case 'toque': state.prefs.toque = v; salvaPrefs(); renderView(); break
    case 'minimizar': janela.minimize(); break
    case 'fechar': janela.close(); break
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
  else if (t.id === 'msg-in') { state.drafts[state.open] = t.value; $('#sendbtn').disabled = !t.value.trim(); autosize() }
  else if (t.id === 'dial-main') { state.dial = t.value.slice(0, 30); renderMatch() }
  else if (t.id === 'dial-transfer') state.dialT = t.value.slice(0, 30)
  else if (t.id === 'vol-toque') { state.prefs.volumeToque = +t.value; $('#vol-toque-v').textContent = t.value + '%'; clearTimeout(volT); volT = setTimeout(salvaPrefs, 400) }
  else if (t.dataset.f && state.form) state.form[t.dataset.f] = t.value
})
document.addEventListener('keydown', e => {
  const t = e.target
  if (e.key === 'Escape' && !$('#pop').hidden) { const a = popAnchor; closePop(); if (a) a.focus(); return }
  if (t.id === 'msg-in' && e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(); return }
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
  try { aplica(await invoke('estado')) } catch (e) { toast('Não li o estado: ' + erroDe(e)) }
  if (!state.contas.length) { go('ajustes'); novoForm(null); renderView() } else go('chamadas')
}
abre()
