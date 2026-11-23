#include "traits.h"
#include "mine.h"
#include "spec.h"
#include "conv.h"
#include "unused.h"
#include "pick.h"
#include "pick_int.h"

int mine_width() { return width<Mine>(); }
int convert() { return conv(1); }
int user() { return pick<int>(); }
