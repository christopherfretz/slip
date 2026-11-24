#include "new_dep.h"
namespace acme { int g(int); }
int u() { using namespace acme; namespace L = acme; return g(1) + L::g(2); }
