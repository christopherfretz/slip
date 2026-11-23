#include "dep.h"
#include "acme.h"

namespace acme {
struct Mine {};
int f(int) { return 1; }
}

long user() { acme::Mine m; return acme::f(1) + acme::g(); }
long acme::g() { return f(1); }
