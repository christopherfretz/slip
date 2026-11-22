#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-spaceship-reversed`: the reversed `y <=> x` against `x <=> y`.
struct R { int r; };
bool operator<(R, int); bool operator<(int, R);
R operator<=>(int, R);
struct X { int i; }; struct Y { int j; };
R operator<=>(X, Y);
X x; Y y;
#ifdef B
R v = x <=> y;
#else
R v = y <=> x;
#endif
