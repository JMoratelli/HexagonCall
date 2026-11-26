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

const $ = s => document.querySelector(s)
const esc = s => String(s == null ? '' : s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c])
const norm = s => String(s).normalize('NFD').replace(/\p{M}/gu, '').toLowerCase()
const erroDe = e => String(e && e.message ? e.message : e)

const PHONE = 'M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z'
const MIC = '<rect x="9" y="2" width="6" height="12" rx="3"/><path d="M19 10v1a7 7 0 0 1-14 0v-1M12 18v4"/>'
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
  back: '<path d="M20 5H9l-7 7 7 7h11a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2Z"/><path d="m18 9-6 6M12 9l6 6"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  alert: '<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4M12 17h.01"/>',
  signal: '<path d="M4 20v-3M9 20v-7M14 20V9M19 20V5"/>',
  caret: '<path d="m6 9 6 6 6-6"/>',
  edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>',
  trash: '<path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>'
}
const ic = (n, cls) => '<svg class="' + (cls || 'ic') + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + ICONS[n] + '</svg>'
const MARK = '<svg viewBox="0 0 28 28" aria-hidden="true"><path d="M8 3.5h12l6 10.5-6 10.5H8L2 14z" fill="var(--art-a)"/><path d="M10.5 9h7l3.5 5-3.5 5h-7L7 14z" fill="var(--art-b)"/></svg>'

const VIEWS = ['chamadas', 'conversas', 'ramais', 'ajustes']
const state = {
  view: 'chamadas', contas: [], prefs: { marca: 'mcalls', microfone: null, altoFalante: null, trazerParaFrente: true },
  temChaveiro: true, pasta: '', hist: [], convs: [], nomes: {},
  call: null, panel: 'call', dial: '', hseg: 'all', missedNew: 0,
  open: null, cq: '', drafts: {}, naoLidas: {},
  form: null, remover: null, devices: null, devErro: null
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
  $('#abar').innerHTML = '<span class="brand">' + MARK + '<span>' + brandName() + '</span></span>' +
    '<nav class="segs" aria-label="Abas">' + tabs.map(([v, label, n, tone]) => '<button type="button" class="seg" data-act="go" data-v="' + v + '"' + (state.view === v ? ' aria-current="page"' : '') + '>' + label + (n ? '<span class="n" style="--c:' + tone + '">' + n + '</span>' : '') + '</button>').join('') + '</nav>' +
    status +
    '<button type="button" class="iconbtn" data-act="gear" aria-label="Configurações" title="Configurações" aria-pressed="' + (state.view === 'ajustes') + '">' + ic('gear') + '</button>'
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
    tx = ic('phone') + '<span class="tx"><b>' + esc(d.n) + '</b> <span>' + esc(c.r) + (c.held ? ' · em espera' : c.deles ? ' · ' + esc(primeiro(d)) + ' pôs em espera' : '') + '</span></span><span class="strip-timer" id="strip-timer">' + fmt((Date.now() - c.t0) / 1000) + '</span>'
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
  else html = callPanel(c)
  el.innerHTML = '<div class="lattice" aria-hidden="true"></div>' + html
  focusDial()
}
function focusDial () {
  if (!finePointer() || state.view !== 'chamadas') return
  const inp = $('#dial-main')
  if (inp && document.activeElement !== inp) { inp.focus({ preventScroll: true }); const n = inp.value.length; inp.setSelectionRange(n, n) }
}
const KEYS = [['1', ''], ['2', 'ABC'], ['3', 'DEF'], ['4', 'GHI'], ['5', 'JKL'], ['6', 'MNO'], ['7', 'PQRS'], ['8', 'TUV'], ['9', 'WXYZ'], ['*', ''], ['0', '+'], ['#', '']]
const padHTML = () => '<div class="pad">' + KEYS.map(([k, l]) => '<button type="button" class="key" data-act="dialkey" data-v="' + k + '" aria-label="' + k + '" tabindex="-1"><b>' + k + '</b><small>' + l + '</small></button>').join('') + '</div>'
function dialPanel () {
  return '<label class="dial-disp"><input id="dial-main" type="text" autocomplete="off" spellcheck="false" aria-label="Ramal, número ou nome" placeholder="Ramal, número ou nome" value="' + esc(state.dial) + '">' +
    '<button type="button" class="iconbtn sm" data-act="dialback" aria-label="Apagar o último" title="Apagar" tabindex="-1">' + ic('back') + '</button></label>' +
    '<div class="match" id="match-main">' + matchHTML() + '</div>' + padHTML() +
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
  return '<div class="cp">' + who + '<div><div class="cp-timer" id="cp-timer">' + fmt((Date.now() - c.t0) / 1000) + '</div><div class="cp-q">' + ic('signal') + esc(c.codec || 'G.711') + ' · 8 kHz' + notas + '</div></div>' +
    '<div class="cp-grid">' +
    '<button type="button" class="cpb" data-act="mute" aria-pressed="' + !!c.muted + '">' + ic(c.muted ? 'micoff' : 'mic') + 'Mudo</button>' +
    '<button type="button" class="cpb" data-act="hold" aria-pressed="' + !!c.held + '">' + ic('pause') + 'Espera</button></div></div>' +
    '<button type="button" class="hangbtn" data-act="hangup">' + ic('hang') + 'Desligar</button>'
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
  const nomeDev = (lista, id) => { const d = (lista || []).find(x => x.id === id); return d ? d.nome : 'Padrão do sistema' }
  const pick = (k, titulo, lista) => '<button type="button" class="chip" data-act="pick" data-k="' + k + '" data-pop aria-haspopup="dialog" aria-expanded="false" title="' + titulo + '"><span>' + esc(nomeDev(lista, p[k])) + '</span>' + ic('caret', 'ic caret') + '</button>'
  $('#content').innerHTML = '<div class="cfgwrap">' +
    '<div class="csec">Contas</div>' + contas +
    '<div class="csec">Áudio</div>' +
    (state.devErro ? crow('Dispositivos', '', '<span class="erro">' + esc(state.devErro) + '</span>') : '') +
    crow('Microfone', '', pick('microfone', 'Microfone', devs && devs.microfones)) +
    crow('Voz da ligação', 'Onde se ouve quem está do outro lado.', pick('altoFalante', 'Voz da ligação', devs && devs.altoFalantes)) +
    '<div class="csec">Janela</div>' +
    crow('Trazer para a frente', 'Quando chega ligação, a janela aparece na frente das outras.', sw('trazerParaFrente', p.trazerParaFrente, 'Trazer a janela para a frente')) +
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
async function acao (cmd, args, falha) {
  try { await invoke(cmd, args); return true } catch (e) { toast(falha + ': ' + erroDe(e)); return false }
}

// Toque discreto: duas notas baixas a cada 3 s, so enquanto chama.
let toque = null
function comecaToque () {
  paraToque()
  try {
    const ctx = new AudioContext()
    const nota = (f, t) => { const o = ctx.createOscillator(), g = ctx.createGain(); o.frequency.value = f; g.gain.setValueAtTime(0, ctx.currentTime + t); g.gain.linearRampToValueAtTime(0.06, ctx.currentTime + t + 0.02); g.gain.linearRampToValueAtTime(0, ctx.currentTime + t + 0.22); o.connect(g).connect(ctx.destination); o.start(ctx.currentTime + t); o.stop(ctx.currentTime + t + 0.25) }
    const tocar = () => { nota(660, 0); nota(880, 0.28) }
    tocar()
    toque = { ctx, t: setInterval(tocar, 3000) }
  } catch (e) { toque = null }
}
function paraToque () { if (toque) { clearInterval(toque.t); toque.ctx.close().catch(() => {}); toque = null } }

// ---- Avisos do telefone ----
function aplica (e) {
  state.contas = e.contas; state.prefs = e.preferencias; state.temChaveiro = e.temChaveiro; state.pasta = e.pasta
  state.hist = e.historico; state.convs = e.conversas
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
      state.panel = 'call'; comecaToque()
      if (state.view === 'chamadas') renderLeft(); renderStrip()
      break
    case 'tocando':
      if (daChamada(a.id)) { state.call.tocando = true; if (state.view === 'chamadas') renderLeft(); renderStrip() }
      break
    case 'atendida':
      if (daChamada(a.id) || (state.call && !state.call.id && state.call.kind === 'outgoing')) {
        paraToque()
        Object.assign(state.call, { id: a.id, kind: 'active', t0: Date.now(), codec: a.codec })
        if (state.view === 'chamadas') renderLeft(); renderStrip()
      }
      break
    case 'espera':
      if (daChamada(a.id)) { state.call.deles = a.sim; if (state.view === 'chamadas') renderLeft(); renderStrip() }
      break
    case 'encerrada': {
      state.hist.push(a.item)
      if (daChamada(a.id)) { paraToque(); state.call = null }
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
      if (!vendo) toast(pessoa(m.ramal).n + ': ' + m.texto)
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
let toastT
function toast (msg) { const t = $('#toast'); t.textContent = msg; t.hidden = false; clearTimeout(toastT); toastT = setTimeout(() => { t.hidden = true }, 3800) }

// ---- Telas ----
function renderView () { ({ chamadas: renderChamadas, conversas: renderConversas, ramais: renderRamais, ajustes: renderAjustes })[state.view]() }
function refresh () { renderChrome(); renderStrip(); renderView() }
function go (v) {
  state.view = VIEWS.includes(v) ? v : 'chamadas'
  closePop()
  if (state.view === 'ajustes' && !state.devices) carregaDispositivos()
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
    case 'dialkey': state.dial = (state.dial + v).slice(0, 30); { const i = $('#dial-main'); if (i) i.value = state.dial } renderMatch(); focusDial(); break
    case 'dialback': state.dial = state.dial.slice(0, -1); { const i = $('#dial-main'); if (i) i.value = state.dial } renderMatch(); focusDial(); break
    case 'dialpick': state.dial = v; { const i = $('#dial-main'); if (i) i.value = v } renderMatch(); focusDial(); break
    case 'dialgo': {
      const t = dialTarget()
      if (!t) { toast(state.dial.trim() ? 'Ninguém com esse nome por aqui. Digite o ramal.' : 'Digite um ramal ou número.'); break }
      startCall(t); break
    }
    case 'answer':
      if (c && c.kind === 'incoming') { paraToque(); if (!await acao('atende', { id: c.id }, 'Não atendi')) comecaToque() }
      break
    case 'decline': if (c) { paraToque(); await acao('desliga', { id: c.id }, 'Não recusei') } break
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
      state.prefs[k] = !state.prefs[k]; b.setAttribute('aria-checked', !!state.prefs[k]); salvaPrefs(); break
    }
    case 'marca': state.prefs.marca = v; salvaPrefs(); refresh(); break
    case 'pick': {
      const k = b.dataset.k
      const lista = state.devices ? (k === 'microfone' ? state.devices.microfones : state.devices.altoFalantes) : []
      const atual = state.prefs[k]
      const opcoes = lista.length ? lista : [{ id: null, nome: 'Padrão do sistema' }]
      openPop(b, '<div class="pop-title">' + esc(b.title) + '</div><div role="radiogroup">' +
        opcoes.map(o => '<button type="button" class="opt" role="radio" aria-checked="' + ((o.id === 'alsa:default' && !atual) || o.id === atual) + '" data-act="picked" data-k="' + k + '" data-v="' + esc(o.id || '') + '"><span class="ck">' + ic('check') + '</span><span class="nm">' + esc(o.nome) + '</span></button>').join('') + '</div>', 'pick-' + k)
      break
    }
    case 'picked': state.prefs[b.dataset.k] = v && v !== 'alsa:default' ? v : null; closePop(); salvaPrefs(); renderView(); break
  }
})
document.addEventListener('input', e => {
  const t = e.target
  if (t.id === 'cq') { state.cq = t.value; renderClist() }
  else if (t.id === 'msg-in') { state.drafts[state.open] = t.value; $('#sendbtn').disabled = !t.value.trim(); autosize() }
  else if (t.id === 'dial-main') { state.dial = t.value.slice(0, 30); renderMatch() }
  else if (t.dataset.f && state.form) state.form[t.dataset.f] = t.value
})
document.addEventListener('keydown', e => {
  const t = e.target
  if (e.key === 'Escape' && !$('#pop').hidden) { const a = popAnchor; closePop(); if (a) a.focus(); return }
  if (t.id === 'msg-in' && e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(); return }
  if (t.id === 'cq' && e.key === 'Enter') { const v = t.value.trim(); const primeiroItem = $('#clist .citem'); if (primeiroItem) primeiroItem.click(); else if (v) { state.open = v; state.cq = ''; renderView() } return }
  if (t.id === 'dial-main' && e.key === 'Enter') { e.preventDefault(); const g = document.querySelector('[data-act="dialgo"]'); if (g) g.click(); return }
  if (e.ctrlKey || e.metaKey || e.altKey || state.view !== 'chamadas' || state.call) return
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
