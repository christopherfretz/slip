#include "dep.h"
namespace __attribute__((visibility("hidden"))) acme { int g(int); }
int u() { return acme::g(1); }
