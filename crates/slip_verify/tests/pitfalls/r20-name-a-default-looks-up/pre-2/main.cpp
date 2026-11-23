const int limit = 4;
namespace app {
const int cap = 8;
template <int N = limit> int take() { return N; }
}

int user() { return app::take(); }
