#include "dep.h"
namespace legacy { int g(D); }
namespace acme { int a; }
int u() { return g(legacy::D{}) + acme::a; }
