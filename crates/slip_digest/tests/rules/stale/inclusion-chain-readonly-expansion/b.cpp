#include "dep.h"
namespace acme { int h; }
int u() { return legacy::f(2) + dep::g() + acme::h; }
