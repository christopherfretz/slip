int f(int, int);
int y = f(1,
#ifdef X
          2
#else
          3
#endif
);
#ifndef X
int z = f(4, 5);
#endif
