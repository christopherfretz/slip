#define CALL f(1) + 0
int f(int);
int g(int);
int a = g(2);
int b = CALL;
