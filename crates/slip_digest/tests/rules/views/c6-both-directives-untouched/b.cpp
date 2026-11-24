#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
using namespace legacy;
using namespace acme;
int u() { return g(1); }
