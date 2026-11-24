#include "old_dep.h"
#include "new_dep.h"
namespace legacy { int g(int); }
using namespace acme;
int u() { return legacy::g(1); }
