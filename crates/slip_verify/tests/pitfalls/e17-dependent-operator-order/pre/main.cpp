#include "x.h"
#include "eq.h"
#include "extra.h"
#include "same.h"

bool caller() { return same(a::X{}, a::X{}); }
