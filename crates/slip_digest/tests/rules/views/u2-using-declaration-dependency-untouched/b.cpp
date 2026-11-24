#include "new_dep.h"
namespace acme { int g(int); }
using acme::Dep;
int u() { return acme::g(1); }
