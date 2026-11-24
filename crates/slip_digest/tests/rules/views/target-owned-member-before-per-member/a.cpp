#include "dep.h"
namespace acme { int old(int); }
namespace legacy { int g(int); }
int u() { return acme::old(1) + legacy::g(1); }
