#include "dep.h"
long make(long n) { return n; }
namespace acme {
int make(int n) { return n; }
}

#include "client.h"
