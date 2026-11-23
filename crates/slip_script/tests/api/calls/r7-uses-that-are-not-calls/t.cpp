int f(int);
int g(int (*)(int));
int (*p)(int) = &f;
int y = g(f) + f(1);
