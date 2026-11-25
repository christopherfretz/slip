#include "side.h"
// WP-155 fix round (B2): the boundary of rule 2 on the marking side.
// Each of these is marked.
int g();
int a = 1;
thread_local int tlv = 0;
struct Dt { ~Dt(); };
int r = g();
int rd = a;
int *pt = &tlv;
int &tr = tlv;
int cast = (int)1.5;
int arr[2] = {1, 2};
const Dt &rt = Dt();
const int &tmp = 1;
int &rr = *&a;
// The second fix round's probes: a read of a variable is a designator
// only where it is bound to a reference as it is; under a list, a
// parenthesis, a sign or a conversion to another type it is a read.
int ex = g();
long x = 1;
int b1{ex};
int b2 = (ex);
int b3 = +ex;
long b4{x};
const int ci = g();
int b5 = ci;
