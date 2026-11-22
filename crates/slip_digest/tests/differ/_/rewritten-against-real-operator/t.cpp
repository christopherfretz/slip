#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-against-real-operator`: `<` rewritten through `<=>` against a real `operator<`.
struct R { int r; };
struct X { int i; };
bool operator<(R, int);
R operator<=>(X, X);
#ifdef B
bool operator<(X, X);
#endif
X a, b;
bool v = a < b;
