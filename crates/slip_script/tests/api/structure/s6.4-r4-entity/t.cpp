namespace n {
int f(int);
int f(int x) { return x; }
}
int y = n::f(1);
