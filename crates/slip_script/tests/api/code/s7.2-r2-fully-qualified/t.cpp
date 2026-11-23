namespace {
namespace m {
int f(int);
}
}
namespace n {
inline namespace v1 {
int g(int);
}
}
int take(int, int);
int x = take(0, 0);
