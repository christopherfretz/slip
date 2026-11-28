#include "side.h"
// WP-138 item 1: folds over `.*` (non-overloadable) and `=` (member-only)
// have no non-member candidates (`oper_lookups` absent) and are determinate.
#ifdef B
template<class... U> auto a(U... u) { return (u = ...); }
template<class M, class... Q> auto b(M m, Q... q) { return (m .* ... .* q); }
template<class... U> auto c(U... u) -> decltype((u = ...));
template<class M, class... Q> auto d(M m, Q... q) -> decltype((m .* ... .* q));
#else
template<class... T> auto a(T... t) { return (t = ...); }
template<class M, class... P> auto b(M m, P... p) { return (m .* ... .* p); }
template<class... T> auto c(T... t) -> decltype((t = ...));
template<class M, class... P> auto d(M m, P... p) -> decltype((m .* ... .* p));
#endif
