#include "old_dep.h"
namespace acme { int g(int); }
int u() { using namespace acme; return g(1); }
