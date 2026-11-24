#include "dep.h"
namespace acme { int g(int); }
namespace legacy { int h(int); }
int u() { return acme::g(1) + legacy::h(1); }
