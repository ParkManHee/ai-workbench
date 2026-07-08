module.exports = function (api) {
  api.cache(true);
  return {
    presets: ["babel-preset-expo"],
    // reanimated 4는 worklets 플러그인 필수(맨 마지막). 없으면 reanimated 애니메이션이
    // 조용히 no-op → react-native-keyboard-controller 등 reanimated 기반 컴포넌트가 동작 안 함.
    plugins: ["react-native-worklets/plugin"],
  };
};
