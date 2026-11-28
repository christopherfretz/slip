#include "dep.h"
namespace acme { namespace newlib { int x() { return 1; } namespace inner { int y() { return 2; } } } int g() { return 3; } }
int u() { return acme::newlib::x() + acme::newlib::inner::y() + acme::g(); }
