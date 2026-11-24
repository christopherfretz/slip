#include "dep.h"
namespace legacy { int h; }
int u() { return legacy::f(2) + dep::g() + legacy::h; }
