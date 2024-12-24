# RustとWebAssemblyによるゲーム開発 ―安全・高速・プラットフォーム非依存のWebアプリ開発入門

## 種本

[github repo](https://github.com/PacktPublishing/Game-Development-with-Rust-and-WebAssembly)

## 環境構築

```ps1
wasm-pack new walk-the-dog
cd walk-the-dog
npm init vite
# vite-project-name => web
```

## ビルド＆実行

### ビルド

```ps
> ./build.ps1
```

### 開発サーバー起動

```ps
> cd web
> npm run dev -- --open
```

## テスト

local用

```ps
> cargo test
```

web用

```ps
> wasm-pack test --headless --chrome
```
