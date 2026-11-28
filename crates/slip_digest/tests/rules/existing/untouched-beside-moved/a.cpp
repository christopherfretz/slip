#include "dep.h"
namespace acme { int x() { return 1; } }
namespace legacy { int g() { return 2; } }
int u() { return acme::x() + legacy::g(); }
