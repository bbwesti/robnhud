% ═══════════════════════════════════════════════════════════════════════════════
% MARKETSCOPE — Market Regime Detection (Deterministic)
%
% Zero hallucination. Pure logic. No predictions. Only regime classification.
%
% Training_Gate: Human_Review_Required
% Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058
% ═══════════════════════════════════════════════════════════════════════════════

:- module(regime, [
    detect_regime/3,
    regime/1,
    classify_regime/2
]).

:- use_module(library(http/json)).

% ── Regime Classification ─────────────────────────────────────────────────────

%% regime(+Regime)
%  Deterministic regime classification based on technical indicators.
%  Exactly one regime must hold at any time.

regime(bull) :-
    sma_50(SMA50),
    sma_200(SMA200),
    vix(VIX),
    breadth(Breadth),
    SMA50 > SMA200,
    VIX < 20,
    Breadth > 0.6.

regime(bear) :-
    sma_50(SMA50),
    sma_200(SMA200),
    vix(VIX),
    SMA50 < SMA200,
    VIX > 30.

regime(transition) :-
    sma_50(SMA50),
    sma_200(SMA200),
    vix(VIX),
    breadth(Breadth),
    \+ (SMA50 > SMA200, VIX < 20, Breadth > 0.6),
    \+ (SMA50 < SMA200, VIX > 30).

% ── Regime Detection Entry Point ──────────────────────────────────────────────

%% detect_regime(+MarketData, -Regime, -Confidence)
%  Main entry point. Takes market data, returns regime classification.
%  Confidence is always 1.0 (deterministic) or 0.0 (insufficient data).

detect_regime(Data, Regime, Confidence) :-
    (   valid_data(Data)
    ->  (   regime(Regime)
        ->  Confidence = 1.0
        ;   Regime = unknown,
            Confidence = 0.0
        )
    ;   Regime = error,
        Confidence = 0.0
    ).

%% classify_regime(+Regime, -Description)
%  Human-readable regime descriptions.

classify_regime(bull, 'Bullish regime: SMA50 > SMA200, VIX < 20, breadth > 0.6').
classify_regime(bear, 'Bearish regime: SMA50 < SMA200, VIX > 30').
classify_regime(transition, 'Transition regime: no clear bull/bear signal').
classify_regime(unknown, 'Unknown regime: insufficient data').
classify_regime(error, 'Error: invalid market data').

% ── Data Validation ───────────────────────────────────────────────────────────

%% valid_data(+Data)
%  Validates that required indicators are present and numeric.

valid_data(Data) :-
    member(sma_50(SMA50), Data),
    member(sma_200(SMA200), Data),
    member(vix(VIX), Data),
    member(breadth(Breadth), Data),
    number(SMA50),
    number(SMA200),
    number(VIX),
    number(Breadth).

% ── Indicator Sources (Stub — replaced by real data at runtime) ───────────────

:- dynamic sma_50/1, sma_200/1, vix/1, breadth/1.

%% set_indicators(+SMA50, +SMA200, +VIX, +Breadth)
%  Sets indicator values from external data feed.

set_indicators(SMA50, SMA200, VIX, Breadth) :-
    retractall(sma_50(_)),
    retractall(sma_200(_)),
    retractall(vix(_)),
    retractall(breadth(_)),
    assert(sma_50(SMA50)),
    assert(sma_200(SMA200)),
    assert(vix(VIX)),
    assert(breadth(Breadth)).
