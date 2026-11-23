namespace n {
int f(int);
}
namespace m {
int g(int);
}
int a = n::f(1);
namespace n {
int b = f(2);
}
