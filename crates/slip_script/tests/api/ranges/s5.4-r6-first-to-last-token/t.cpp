#define ID(x) x
int f(int);
int v;
int y = f( /* a */ v /* b */ );
int z = f(ID(v));
