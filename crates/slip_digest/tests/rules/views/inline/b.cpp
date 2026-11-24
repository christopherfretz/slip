#include "dep.h"
namespace acme { inline namespace v1 { int g(int); } }
int u() { return acme::g(1); }
