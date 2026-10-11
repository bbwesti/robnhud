% ═══════════════════════════════════════════════════════════════════════════════
% MARKETSCOPE — Sovereign Compliance Guard (Deterministic)
%
% Enforces sovereign constraints at the logic layer.
% All signals must pass compliance before output.
%
% PROHIBITED_ACTION: financial_advice
% Training_Gate: Human_Review_Required
% Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058
% ═══════════════════════════════════════════════════════════════════════════════

:- module(compliance, [
    compliant_signal/1,
    verify_signal/2,
    block_financial_advice/1,
    require_human_review/1
]).

:- use_module(library(http/json)).

% ── Signal Compliance ─────────────────────────────────────────────────────────

%% compliant_signal(+Signal)
%  A signal is compliant ONLY if:
%  1. Source is verified (Ed25519 signature valid)
%  2. Signal is logged to WORM chain
%  3. Signal is NOT financial advice
%  4. Human review gate has passed

compliant_signal(Signal) :-
    source_verified(Signal),
    worm_logged(Signal),
    \+ prohibited_action(Signal),
    human_review_gate_passed(Signal).

%% verify_signal(+Signal, -Result)
%  Verifies compliance and returns detailed result.

verify_signal(Signal, Result) :-
    (   compliant_signal(Signal)
    ->  Result = _{
            signal: Signal,
            compliant: true,
            message: 'Signal passed all compliance checks'
        }
    ;   find_violations(Signal, Violations),
        Result = _{
            signal: Signal,
            compliant: false,
            violations: Violations,
            message: 'Signal failed compliance checks'
        }
    ).

% ── Prohibited Actions ────────────────────────────────────────────────────────

%% prohibited_action(+Signal)
%  Financial advice is PROHIBITED under sovereign axioms.
%  This is a hard constraint — no exceptions.

prohibited_action(Signal) :-
    get_dict(type, Signal, Type),
    member(Type, [buy_signal, sell_signal, price_target, trade_recommendation]).

%% block_financial_advice(+Signal)
%  Explicitly blocks financial advice signals.

block_financial_advice(Signal) :-
    (   prohibited_action(Signal)
    ->  throw(error(prohibited_action, Signal))
    ;   true
    ).

% ── Human Review Gate ─────────────────────────────────────────────────────────

%% human_review_gate_passed(+Signal)
%  All signals require human review before output.
%  This is a Training_Gate constraint.

human_review_gate_passed(Signal) :-
    get_dict(review_status, Signal, Status),
    Status = approved_by_human.

%% require_human_review(+Signal)
%  Marks a signal as requiring human review.

require_human_review(Signal) :-
    put_dict(review_status, Signal, pending_review, Updated),
    worm_log(Updated).

% ── Violation Detection ───────────────────────────────────────────────────────

%% find_violations(+Signal, -Violations)
%  Finds all compliance violations for a signal.

find_violations(Signal, Violations) :-
    findall(V, violation(Signal, V), Violations).

%% violation(+Signal, -Violation)
%  Individual violation checks.

violation(Signal, source_not_verified) :-
    \+ source_verified(Signal).

violation(Signal, not_worm_logged) :-
    \+ worm_logged(Signal).

violation(Signal, financial_advice_prohibited) :-
    prohibited_action(Signal).

violation(Signal, human_review_not_passed) :-
    \+ human_review_gate_passed(Signal).

% ── Data Sources (Stubs — replaced at runtime) ───────────────────────────────

:- dynamic source_verified/1, worm_logged/1, human_review_gate_passed/1.

%% set_source_verified(+Signal, +Verified)
set_source_verified(Signal, Verified) :-
    retractall(source_verified(Signal)),
    (   Verified == true
    ->  assert(source_verified(Signal))
    ;   true
    ).

%% set_worm_logged(+Signal, +Logged)
set_worm_logged(Signal, Logged) :-
    retractall(worm_logged(Signal)),
    (   Logged == true
    ->  assert(worm_logged(Signal))
    ;   true
    ).

%% set_human_review(+Signal, +Approved)
set_human_review(Signal, Approved) :-
    retractall(human_review_gate_passed(Signal)),
    (   Approved == true
    ->  assert(human_review_gate_passed(Signal))
    ;   true
    ).

% ── WORM Logging (Stub — connects to Bifrost SDK) ────────────────────────────

%% worm_log(+Signal)
%  Logs signal to WORM chain. Stub — replaced by Bifrost SDK at runtime.

worm_log(_Signal) :-
    % TODO: Connect to Bifrost WORM chain
    % bifrost:append_block(Signal),
    true.
