#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-lt-reversed`: `x < y` from `<=>` against the reversed `y < x`.
struct R { int r; };
bool operator<(R, int); bool operator<(int, R);
R operator<=>(int, R);
struct X { int i; }; struct Y { int j; };
R operator<=>(X, Y);
X x; Y y;
#ifdef B
bool v = y < x;
#else
bool v = x < y;
#endif
