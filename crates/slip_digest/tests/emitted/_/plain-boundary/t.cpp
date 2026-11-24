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
