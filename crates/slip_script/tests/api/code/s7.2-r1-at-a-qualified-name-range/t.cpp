namespace n {
int f(int);
int g(int);
}
int a = n::f(1);
namespace n {
int b = f(2);
}
