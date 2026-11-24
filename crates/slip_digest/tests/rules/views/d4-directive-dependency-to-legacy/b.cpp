#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
using namespace legacy;
acme::Dep d;
int u() { return acme::g(1); }
