#include "dep.h"
#include "acme.h"

namespace acme {
struct Mine {};
} namespace legacy {
int f(int) { return 1; }
}

long user() { acme::Mine m; return legacy::f(1) + acme::g(); }
long acme::g() { return f(1); }
