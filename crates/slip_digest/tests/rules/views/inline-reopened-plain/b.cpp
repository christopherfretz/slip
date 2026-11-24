#include "dep.h"
namespace acme { namespace v1 { int g(int); } }
int u() { return acme::v1::g(1); }
