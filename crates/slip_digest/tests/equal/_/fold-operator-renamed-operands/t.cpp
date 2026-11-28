#include "side.h"
// WP-138: renamed operand types (template type parameter pack and typedef
// alias) in a fold with no operator found in the definition context: equal.
using P = int;
using Q = int;
#ifdef B
template<class... U> auto f(U... u) { return (Q{0} + ... + u); }
template<class... U> auto g(U... u) -> decltype((Q{0} + ... + u));
#else
template<class... T> auto f(T... t) { return (P{0} + ... + t); }
template<class... T> auto g(T... t) -> decltype((P{0} + ... + t));
#endif
