#include "dep.h"
namespace acme { inline namespace v1 { int k(); } }
int u() { return acme::k(); }
