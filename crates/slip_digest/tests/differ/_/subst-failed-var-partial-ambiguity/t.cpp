#include "side.h"
// WP-136 item 2 kind 3 & item 4 ("Its kin"): variable template whose partial
// specialisation choice is ambiguous under SFINAE on side A
// (`VariablePartialAmbiguity`, no unit) and disambiguated on side B.
template <class T, class U> constexpr int v = 0;
template <class T> constexpr int v<T, int> = 1;
template <class U> constexpr int v<int, U> = 2;
#ifdef B
template <> constexpr int v<int, int> = 3;
#endif
template <class T> auto f(T) -> decltype(v<T, int>);
long f(...);
long x = f(1);
