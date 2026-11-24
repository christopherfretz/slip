#include "wrap.h"
namespace acme { int g(int); }
int u() { return acme::g(1); }
