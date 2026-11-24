#include "old_dep.h"
namespace acme { int g(int); }
namespace L = acme;
int u() { return L::g(1); }
