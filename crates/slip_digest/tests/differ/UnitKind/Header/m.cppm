module;
#include "side.h"
#ifndef B
#include "h.h"
#endif
export module m;
#ifdef B
import "h.h";
#endif
