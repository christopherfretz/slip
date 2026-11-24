#include "new_dep.h"
namespace legacy { int g(int); }
using acme::Dep;
int u() { return legacy::g(1); }
