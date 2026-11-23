namespace n {
int f(int, int);
int g(int);
}
int a = n::f(n::g(1), 2);
