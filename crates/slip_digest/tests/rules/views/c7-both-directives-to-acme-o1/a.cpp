#include "new_dep.h"
namespace legacy { int g(int); }
using namespace legacy;
using namespace acme;
int u() { return g(1); }
