% ═══════════════════════════════════════════════════════════════════════════════
% MARKETSCOPE — Correlation Breakdown Detection (Deterministic)
%
% Detects when historically correlated assets diverge.
% Outputs scenario bands, not predictions.
%
% Training_Gate: Human_Review_Required
% Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058
% ═══════════════════════════════════════════════════════════════════════════════

:- module(correlation, [
    correlation_breakdown/3,
    check_all_pairs/2,
    pair_status/3
]).

:- use_module(library(http/json)).

% ── Correlation Breakdown Detection ───────────────────────────────────────────

%% correlation_breakdown(+AssetA, +AssetB, -Window)
%  Detects when rolling correlation drops below threshold.
%  Window is the lookback period in days.

correlation_breakdown(AssetA, AssetB, Window) :-
    rolling_corr(AssetA, AssetB, Window, Corr),
    historical_avg_corr(AssetA, AssetB, Avg),
    Corr < 0.3,
    Avg > 0.7.

%% correlation_breakdown(+AssetA, +AssetB, -Window, -Details)
%  Extended version with detailed breakdown information.

correlation_breakdown(AssetA, AssetB, Window, Details) :-
    rolling_corr(AssetA, AssetB, Window, CurrentCorr),
    historical_avg_corr(AssetA, AssetB, HistoricalAvg),
    deviation(Deviation, CurrentCorr, HistoricalAvg),
    (   CurrentCorr < 0.3,
        HistoricalAvg > 0.7
    ->  Details = _{
            asset_a: AssetA,
            asset_b: AssetB,
            window_days: Window,
            current_corr: CurrentCorr,
            historical_avg: HistoricalAvg,
            deviation: Deviation,
            alert: true,
            message: 'CORRELATION BREAKDOWN DETECTED'
        }
    ;   Details = _{
            asset_a: AssetA,
            asset_b: AssetB,
            window_days: Window,
            current_corr: CurrentCorr,
            historical_avg: HistoricalAvg,
            deviation: Deviation,
            alert: false,
            message: 'Correlation within normal range'
        }
    ).

% ── Multi-Pair Analysis ──────────────────────────────────────────────────────

%% check_all_pairs(+Pairs, -Alerts)
%  Checks all asset pairs for correlation breakdowns.

check_all_pairs(Pairs, Alerts) :-
    maplist(check_pair, Pairs, Results),
    include(has_alert, Results, Alerts).

%% check_pair(+Pair, -Result)
%  Checks a single pair.

check_pair(AssetA-AssetB, Result) :-
    (   correlation_breakdown(AssetA, AssetB, 20, Details)
    ->  Result = Details
    ;   Result = _{
            asset_a: AssetA,
            asset_b: AssetB,
            alert: false,
            message: 'No breakdown detected'
        }
    ).

%% pair_status(+AssetA, +AssetB, -Status)
%  Returns current correlation status.

pair_status(AssetA, AssetB, Status) :-
    rolling_corr(AssetA, AssetB, 20, Current),
    historical_avg_corr(AssetA, AssetB, Historical),
    (   Current < 0.3, Historical > 0.7
    ->  Status = breakdown
    ;   Current < 0.5
    ->  Status = weakening
    ;   Status = normal
    ).

% ── Helpers ───────────────────────────────────────────────────────────────────

%% deviation(-Deviation, +Current, +Historical)
%  Computes absolute deviation.

deviation(D, Current, Historical) :-
    D is abs(Current - Historical).

%% has_alert(+Details)
%  Checks if a result contains an alert.

has_alert(Details) :-
    get_dict(alert, Details, true).

% ── Data Sources (Stubs — replaced at runtime) ───────────────────────────────

:- dynamic rolling_corr/4, historical_avg_corr/3.

%% set_rolling_corr(+AssetA, +AssetB, +Window, +Corr)
set_rolling_corr(AssetA, AssetB, Window, Corr) :-
    retractall(rolling_corr(AssetA, AssetB, Window, _)),
    assert(rolling_corr(AssetA, AssetB, Window, Corr)).

%% set_historical_avg(+AssetA, +AssetB, +Avg)
set_historical_avg(AssetA, AssetB, Avg) :-
    retractall(historical_avg_corr(AssetA, AssetB, _)),
    assert(historical_avg_corr(AssetA, AssetB, Avg)).
