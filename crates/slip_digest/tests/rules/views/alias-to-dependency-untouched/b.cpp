#include "dep.h"
namespace A = acme;
namespace acme { int g(int); }
int u() { return acme::g(1) + sizeof(A::Dep); }
