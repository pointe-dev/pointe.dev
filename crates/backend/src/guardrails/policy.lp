% pointe.dev abuse guardrails — Answer Set Programming policy (clingo).
%
% Input facts (emitted by facts.rs from the n8n workflow JSON):
%   node(W, Name, Type)               one per node; W = workflow index
%   edge(W, From, To)                 a connection From -> To
%   trigger_interval(W, Node, Secs)   a time trigger firing every Secs seconds
%   http_out(W, Node, Domain, Method) an HTTP request node (Domain="unknown" if dynamic)
%   email_out(W, Node)                a node that sends email
%   loop_node(W, Node, Batch)         a batching/loop node (Batch=0 if unbounded/unknown)
%
% Output: violation(Kind, W, Node) — derived, deterministic. One answer set.
% This program never makes choices, so the violations are a pure function of the facts.

% ── Reachability (transitive closure over connections) ─────────────────────────
reaches(W, A, B) :- edge(W, A, B).
reaches(W, A, C) :- edge(W, A, B), reaches(W, B, C).

% ── Outbound classification: nodes that act on the outside world ───────────────
outbound(W, N) :- http_out(W, N, _, _).
outbound(W, N) :- email_out(W, N).

% A trigger is "high frequency" below 5 minutes.
high_freq(W, T) :- trigger_interval(W, T, S), S < 300.

% ── Rule: FLOOD ────────────────────────────────────────────────────────────────
% A high-frequency trigger that reaches an outbound node = potential spam/flood
% (e.g. "send an email every minute", "ping this endpoint every 30s").
violation(flood, W, N) :- high_freq(W, T), reaches(W, T, N), outbound(W, N).
violation(flood, W, T) :- high_freq(W, T), outbound(W, T).

% ── Rule: MASS_POST ──────────────────────────────────────────────────────────
% A loop/batch node that reaches an external HTTP POST = potential mass submission
% / form-stuffing (e.g. "submit this form 100 times").
violation(mass_post, W, N) :- loop_node(W, L, _), reaches(W, L, N),
                              http_out(W, N, _, "POST").

% ── Rule: SCRAPE_LOOP ────────────────────────────────────────────────────────
% A loop reaching repeated external HTTP GETs = potential scraping at volume.
violation(scrape_loop, W, N) :- loop_node(W, L, _), reaches(W, L, N),
                                http_out(W, N, D, "GET"), D != "unknown".

#show violation/3.
