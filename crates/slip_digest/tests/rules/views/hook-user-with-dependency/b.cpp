#include "dep.h"
namespace acme { int g(int); }
int u() { return acme::g(1) + legacy::hook(1); }
