int f(int, int);
int g(int);
int y = f(g( 1 ), (2 + 3));
int z = f(y, y)
    + 1;
