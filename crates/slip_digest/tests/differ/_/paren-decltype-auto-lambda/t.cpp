#include "side.h"
// §7 "Parentheses" (F-B): the same for a generic lambda with `-> decltype(auto)`.
int x;
#ifdef B
inline auto l = [](auto) -> decltype(auto) { return (x); };
#else
inline auto l = [](auto) -> decltype(auto) { return x; };
#endif
