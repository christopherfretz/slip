#include "side.h"
#ifdef B
namespace acme { template <class T> struct S {}; int g(int); }
#include "spec.h"
using namespace acme;
#else
namespace legacy { template <class T> struct S {}; int g(int); }
#include "spec.h"
using namespace legacy;
#endif
int u() { return g(1) + sizeof(S<int>); }
