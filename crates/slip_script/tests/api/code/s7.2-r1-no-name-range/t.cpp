namespace n {
int f(int);
int g(int);
}
#define NAME f
int a = n::NAME(1);
