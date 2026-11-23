#include "dep.h"
int make(long) { return 2; }
namespace legacy {
int make(int) { return 1; }
}

using namespace legacy;

#include "client.h"
