#include "side.h"
// WP-155: a static local is initialised on first pass through its
// declaration, not before `main`: not marked (nor its function).
int g();
int f() { static int n = g(); return n; }
template <class T> int t() { static int m = g(); return m; }
int y() { return t<int>(); }
