#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-eq-reversed`: the reversed `Y{} == X(1)` (rewritten) against `X(1) == Y{}` (the call itself).
struct X { X(int); }; struct Y {};
bool operator==(X, Y);
#ifdef B
bool v = X(1) == Y{};
#else
bool v = Y{} == X(1);
#endif
