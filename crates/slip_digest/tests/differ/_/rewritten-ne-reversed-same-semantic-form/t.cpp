#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-ne-reversed-same-semantic-form`: `!(X(1) == Y{})` both, written `X(1) != Y{}` against the reversed `Y{} != X(1)`.
struct X { X(int); }; struct Y {};
bool operator==(X, Y);
#ifdef B
bool v = X(1) != Y{};
#else
bool v = Y{} != X(1);
#endif
