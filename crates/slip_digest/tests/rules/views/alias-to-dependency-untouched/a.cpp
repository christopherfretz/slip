#include "dep.h"
namespace A = acme;
namespace legacy { int g(int); }
int u() { return legacy::g(1) + sizeof(A::Dep); }
