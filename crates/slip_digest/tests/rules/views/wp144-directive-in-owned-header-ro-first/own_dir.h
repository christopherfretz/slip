#pragma once
#include "std.h"
#ifdef B
namespace acme { using namespace std; }
#else
namespace legacy { using namespace std; }
#endif
