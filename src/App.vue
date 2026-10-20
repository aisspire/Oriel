<script setup lang="ts">
import AppIcon from './components/AppIcon.vue'
import OrielShowcase from './components/OrielShowcase.vue'
import { useAppearance } from './composables/useAppearance'

const { theme, reducedMotion, paused, motionPaused, toggleTheme } = useAppearance()
</script>

<template>
  <a class="skip-link" href="#home">跳到首页内容</a>
  <div class="app-shell">
    <aside class="sidebar" aria-label="主侧边栏">
      <a class="brand-link" href="#home" aria-label="Oriel 首页">
        <img src="/oriel-icon.svg" alt="" width="44" height="44" />
      </a>

      <span class="sidebar-divider" aria-hidden="true"></span>

      <nav class="primary-nav" aria-label="主导航">
        <a class="nav-link is-active" href="#home" aria-current="page" aria-label="首页">
          <AppIcon name="home" />
          <span class="nav-tooltip">首页</span>
        </a>
      </nav>

      <div class="sidebar-bottom">
        <button
          class="icon-button theme-toggle"
          type="button"
          :aria-label="theme === 'light' ? '切换为深色外观' : '切换为浅色外观'"
          :title="theme === 'light' ? '切换为深色外观' : '切换为浅色外观'"
          @click="toggleTheme"
        >
          <AppIcon :name="theme === 'light' ? 'moon' : 'sun'" />
        </button>
        <span class="sidebar-edition" aria-label="版本 0.1">0.1</span>
      </div>
    </aside>

    <main id="home" class="workspace" tabindex="-1">
      <header class="topbar">
        <div class="brand-wordmark" aria-label="Oriel 个人工作台">
          <span>oriel</span>
          <span class="wordmark-divider" aria-hidden="true"></span>
          <span class="workspace-label">个人工作台</span>
        </div>
        <div class="home-indicator">
          <span class="indicator-dot" aria-hidden="true"></span>
          默认首页
        </div>
      </header>

      <div class="page-heading">
        <div>
          <p class="page-eyebrow">属于你的，刚刚好。</p>
          <h1>欢迎回到 Oriel<span class="greeting-dot">.</span></h1>
        </div>
        <p class="heading-note">留一点空间，<br />给下一次灵感。</p>
      </div>

      <div class="home-grid">
        <OrielShowcase
          :paused="motionPaused"
          :reduced-motion="reducedMotion"
          @toggle-motion="paused = !paused"
        />

        <section class="focus-card panel" aria-labelledby="focus-title">
          <div class="card-topline">
            <span class="card-label">少一点纷扰</span>
            <AppIcon name="spark" />
          </div>
          <h2 id="focus-title">专注于<br />真正重要的事。</h2>
          <p>让工具各就其位，<br />让思绪自由生长。</p>
          <div class="focus-art" aria-hidden="true">
            <div class="focus-orbit"></div>
            <div class="focus-petal petal-mint"></div>
            <div class="focus-petal petal-peach"></div>
            <div class="focus-petal petal-blue"></div>
            <div class="focus-petal petal-lilac"></div>
            <span class="focus-center"></span>
          </div>
          <span class="focus-footnote">A LITTLE ROOM TO FOCUS</span>
        </section>

        <section class="space-card panel" aria-labelledby="space-title">
          <div class="space-copy">
            <span class="card-label">从容开始</span>
            <h2 id="space-title">留白，也很美。</h2>
            <p>一个安静的起点，无限可能。</p>
          </div>
          <div class="swatch-stack" aria-hidden="true">
            <span class="swatch swatch-blue"></span>
            <span class="swatch swatch-peach"></span>
            <span class="swatch swatch-mint"><AppIcon name="window" /></span>
          </div>
        </section>
      </div>

      <section class="tools-space panel" aria-labelledby="tools-title">
        <div class="tools-intro">
          <div class="empty-symbol" aria-hidden="true">
            <AppIcon name="window" />
          </div>
          <div>
            <div class="tools-title-row">
              <h2 id="tools-title">你的工具空间</h2>
              <span class="tool-count" aria-label="0 个工具">0</span>
            </div>
            <p>这里还没有工具。让每一个加入的，都恰好是你需要的。</p>
          </div>
        </div>
        <span class="empty-caption">好用的日常，从这里生长。</span>
      </section>

      <footer class="workspace-footer">
        <span>Oriel <span class="footer-dot" aria-hidden="true">·</span> 为你的日常，打开一扇窗。</span>
        <span class="footer-palette" aria-hidden="true">
          <i></i><i></i><i></i><i></i>
        </span>
      </footer>
    </main>
  </div>
</template>
