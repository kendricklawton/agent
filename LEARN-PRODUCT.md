# LEARN-PRODUCT.md — mastering the financial domain

A personal roadmap for the **domain knowledge** this engine serves. You can't design a good canonical
schema (or judge whether the LLM's answer is *right*) without understanding the instruments underneath. Work
top-to-bottom; each tier says **what to learn**, **why it matters for the engine**, and **where to learn it**.

> **North star:** understand markets well enough to (1) model the entities the engine returns, (2) know
> which questions users will ask, and (3) recognize a wrong answer. This is a **research/analysis** tool —
> never financial advice.

---

## Tier 0 — Market mechanics (the ground floor)
- [ ] What a market is: buyers/sellers, exchanges, brokers, market makers, clearing/settlement
- [ ] The **order book**: bid/ask, spread, depth, liquidity; market vs limit orders
- [ ] **Trades vs quotes**: a *trade* is an execution; a *quote* is the current bid/ask (NBBO)
- [ ] Trading sessions: regular/pre/post market, market hours, holidays, **timezones** (everything is US/Eastern-ish but store UTC)
- [ ] Price vs return; notional value; long vs short

**Why here:** these become the primitives of your canonical schema (`Trade`, `Quote`, `Instrument`) and the
sessions/timezone rules that bite every data adapter.
**Learn:** Investopedia "Market Basics"; *Trading and Exchanges* (Larry Harris) for depth (skim early).

## Tier 1 — Equities & ETFs
- [ ] Shares, market cap, float; tickers & listing exchanges
- [ ] **Corporate actions**: splits, dividends, mergers — and **adjusted vs unadjusted** prices ⚠️
- [ ] OHLCV **bars/aggregates** (open/high/low/close/volume) at intervals; ticks; VWAP
- [ ] ETFs, indices (S&P 500, etc.), and how an index differs from a tradable instrument

**Why here:** adjusted-vs-unadjusted is the #1 source of "the numbers are wrong" bugs — the engine must be
explicit about which it returns. Bars/quotes/trades are your core time-series types.
**Learn:** Investopedia; Polygon's data model docs (their split/dividend/aggregates endpoints are a great
concrete reference for schema design).

## Tier 2 — Options ⭐ (the hardest, highest-leverage instrument)
- [ ] Calls & puts; **strike, expiration, premium**; American vs European exercise
- [ ] Intrinsic vs extrinsic value; **moneyness** (ITM/ATM/OTM)
- [ ] The **Greeks**: delta, gamma, theta, vega, rho — what each measures
- [ ] **Implied volatility** and the vol surface; why IV ≠ realized vol
- [ ] **Option chains** and **OCC option symbology** (root+expiry+type+strike)
- [ ] Core strategies: covered call, verticals/spreads, straddles/strangles

**Why here:** options are structurally richer than stocks — an `OptionContract` needs strike/expiry/type/IV/
Greeks, and the option *chain* is its own query shape. Get this right and futures/prediction markets are easy.
**Learn:** **Hull, *Options, Futures, and Other Derivatives*** (the standard); Natenberg, *Option Volatility
& Pricing*; CBOE Options Institute (free); tastylive for intuition.

## Tier 3 — Futures
- [ ] Contracts: underlying, **expiration**, contract size/notional, tick size
- [ ] **Settlement**: cash vs physical; margin (initial/maintenance)
- [ ] **Contango vs backwardation**; the **roll** and continuous contracts
- [ ] Categories: index, commodity, rate, FX futures

**Why here:** futures add expiry/roll and margin to the model; "continuous contract" is a data-modeling
decision your provider adapter must make explicit.
**Learn:** Hull (same book); CME Institute (free, excellent).

## Tier 4 — Prediction markets & the rest
- [ ] **Prediction markets (Kalshi)**: event/YES-NO contracts, **price ≈ implied probability**, settlement
- [ ] How a prediction contract differs from an option (binary payoff, event-settled)
- [ ] Brief: FX, crypto, bonds (yield, coupon, duration) — enough to model, not master

**Why here:** the prediction-market angle is your differentiator — "LLM-estimated probability vs
market-implied probability" is a query shape nobody's nailed. Model an `Event`/`Market` with an implied
probability.
**Learn:** Kalshi's docs + help center; a light macro/fixed-income primer only if you add those providers.

## Tier 5 — Market **data** (the bridge to the engine) ⭐
- [ ] Data kinds: **trades, quotes, aggregates/bars, snapshots, reference data, fundamentals, news**
- [ ] Level 1 vs Level 2; historical vs real-time; **streaming** (websockets/SSE) vs REST
- [ ] **Symbology**: ticker vs CUSIP vs FIGI vs OCC option symbols — and why you need a canonical ID
- [ ] Trading **calendars/holidays**, bar alignment, timezones, and gaps
- [ ] Corporate-action adjustment pipelines; data licensing & redistribution limits ⚠️

**Why here:** *this tier is literally your canonical schema.* Every entity here becomes a typed core model
that adapters map onto. Symbology + adjustments + calendars are where providers disagree — i.e., where drift
lives.
**Learn:** Polygon and Kalshi API docs read cover-to-cover; note where two providers model the *same* thing
differently — that gap is what your canonical schema exists to hide.

## Tier 6 — Analysis (what users will actually ask)
- [ ] Returns, volatility, correlation; moving averages
- [ ] Common indicators: RSI, MACD, Bollinger Bands, volume profile
- [ ] Fundamentals: earnings, P/E, revenue, the three financial statements
- [ ] Basic probability/statistics for markets (distributions, expected value)

**Why here:** these are the *questions* ("what's NVDA's 20-day volatility?", "is AAPL overbought?"). Knowing
them tells you which computed fields the engine should support and lets you verify LLM answers.
**Learn:** Investopedia technical-analysis section; any intro quant-finance course.

## Tier 7 — From domain → canonical schema
Turn everything above into the engine's typed entities. A first cut:
- [ ] `Instrument` (equity / option / future / event) + canonical `InstrumentId`
- [ ] `Trade`, `Quote`, `Bar` (with an explicit `adjusted: bool` and interval)
- [ ] `OptionContract` (+ Greeks/IV) and `OptionChain`
- [ ] `FuturesContract` (expiry, continuous?), `Event`/`Market` (implied probability)
- [ ] `CorporateAction`, `Fundamental`, `News`
- [ ] **Capability descriptor**: which of these a given provider can serve

**Why here:** this is the deliverable — it feeds directly into `LEARN-TECHNICAL.md`'s schema/adapter work.

## Tier 8 — Compliance & ethics (don't skip)
- [ ] "Research/analysis, not financial advice" — and why that framing matters legally
- [ ] Market-data **licensing/redistribution** rules (you can't just re-serve provider data)
- [ ] Never fabricate numbers; always ground answers in fetched data and cite the source

---

## The bookshelf (curated, not exhaustive)
- **Hull — *Options, Futures, and Other Derivatives*** — the one derivatives reference.
- **Larry Harris — *Trading and Exchanges*** — market microstructure.
- **Natenberg — *Option Volatility & Pricing*** — options intuition.
- **Investopedia** — fast lookups for any term.
- **CME Institute** & **CBOE Options Institute** — free, high-quality, instrument-specific.
- **Polygon** & **Kalshi** API docs — your concrete data-model references.

> Pace: Tiers 0–2 and 5 are the critical path for a first version (equities + options + the data model).
> Futures, prediction markets, and deep analysis can come as you add providers and query types.
