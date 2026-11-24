#include "dep.h"
namespace acme { int g(legacy::D); int a; }
int u() { return acme::g(legacy::D{}) + acme::a; }
