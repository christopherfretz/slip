#include "dep.h"
#include "acme.h"

namespace legacy {
struct Mine {};
int f(int) { return 1; }
}

long user() { legacy::Mine m; return legacy::f(1) + acme::g(); }
long acme::g() { return f(1); }
