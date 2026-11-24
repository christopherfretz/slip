#include "new_dep.h"
using namespace acme;
namespace acme { int g(int); }
int u() { Dep d; return acme::g(1); }
