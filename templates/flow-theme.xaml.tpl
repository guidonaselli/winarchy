<!-- managed by winarchy — generado desde templates/flow-theme.xaml.tpl. NO editar a mano. Theme: {{name}} -->
<ResourceDictionary xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
    <ResourceDictionary.MergedDictionaries>
        <ResourceDictionary Source="pack://application:,,,/Themes/Base.xaml" />
    </ResourceDictionary.MergedDictionaries>
    <Thickness x:Key="ResultMargin">0 0 0 6</Thickness>
    <Style x:Key="QueryBoxStyle" BasedOn="{StaticResource BaseQueryBoxStyle}" TargetType="{x:Type TextBox}">
        <Setter Property="Background" Value="{{colors.background}}" />
        <Setter Property="Foreground" Value="{{colors.foreground}}" />
        <Setter Property="CaretBrush" Value="{{colors.cursor}}" />
        <Setter Property="SelectionBrush" Value="{{colors.color0}}" />
    </Style>
    <Style x:Key="QuerySuggestionBoxStyle" BasedOn="{StaticResource BaseQuerySuggestionBoxStyle}" TargetType="{x:Type TextBox}">
        <Setter Property="Background" Value="{{colors.background}}" />
        <Setter Property="Foreground" Value="{{colors.color8}}" />
    </Style>
    <Style x:Key="WindowBorderStyle" BasedOn="{StaticResource BaseWindowBorderStyle}" TargetType="{x:Type Border}">
        <Setter Property="Background" Value="{{colors.background}}" />
        <Setter Property="BorderBrush" Value="{{colors.accent}}" />
        <Setter Property="BorderThickness" Value="1" />
        <Setter Property="CornerRadius" Value="8" />
    </Style>
    <Style x:Key="WindowStyle" BasedOn="{StaticResource BaseWindowStyle}" TargetType="{x:Type Window}" />
    <Style x:Key="PendingLineStyle" BasedOn="{StaticResource BasePendingLineStyle}" TargetType="{x:Type Line}">
        <Setter Property="Stroke" Value="{{colors.accent}}" />
    </Style>
    <Style x:Key="ItemTitleStyle" BasedOn="{StaticResource BaseItemTitleStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.foreground}}" />
    </Style>
    <Style x:Key="ItemSubTitleStyle" BasedOn="{StaticResource BaseItemSubTitleStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.color8}}" />
    </Style>
    <Style x:Key="ItemTitleSelectedStyle" BasedOn="{StaticResource BaseItemTitleSelectedStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.accent}}" />
    </Style>
    <Style x:Key="ItemSubTitleSelectedStyle" BasedOn="{StaticResource BaseItemSubTitleSelectedStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.foreground}}" />
    </Style>
    <Style x:Key="ItemHotkeyStyle" BasedOn="{StaticResource BaseItemHotkeyStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.color8}}" />
    </Style>
    <Style x:Key="ItemHotkeySelectedStyle" BasedOn="{StaticResource BaseItemHotkeySelectedStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.accent}}" />
    </Style>
    <Style x:Key="HighlightStyle">
        <Setter Property="TextElement.Foreground" Value="{{colors.accent}}" />
    </Style>
    <SolidColorBrush x:Key="ItemSelectedBackgroundColor">{{colors.color0}}</SolidColorBrush>
    <Style x:Key="ItemGlyph" BasedOn="{StaticResource BaseGlyphStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.foreground}}" />
    </Style>
    <Style x:Key="ItemNumberStyle" BasedOn="{StaticResource BaseItemNumberStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.color8}}" />
    </Style>
    <Style x:Key="ItemImageSelectedStyle" BasedOn="{StaticResource BaseItemImageSelectedStyle}" TargetType="{x:Type Image}">
        <Setter Property="Cursor" Value="Arrow" />
    </Style>
    <Style x:Key="ThumbStyle" BasedOn="{StaticResource BaseThumbStyle}" TargetType="{x:Type Thumb}">
        <Setter Property="Width" Value="2" />
        <Setter Property="Template">
            <Setter.Value>
                <ControlTemplate TargetType="{x:Type Thumb}">
                    <Border Background="{{colors.color8}}" BorderThickness="0" CornerRadius="2" DockPanel.Dock="Right" />
                </ControlTemplate>
            </Setter.Value>
        </Setter>
    </Style>
    <Style x:Key="ScrollBarStyle" BasedOn="{StaticResource BaseScrollBarStyle}" TargetType="{x:Type ScrollBar}" />
    <Style x:Key="SeparatorStyle" BasedOn="{StaticResource BaseSeparatorStyle}" TargetType="{x:Type Rectangle}">
        <Setter Property="Fill" Value="{{colors.color0}}" />
        <Setter Property="Height" Value="1" />
        <Setter Property="Margin" Value="12 0 12 6" />
    </Style>
    <Style x:Key="SearchIconStyle" BasedOn="{StaticResource BaseSearchIconStyle}" TargetType="{x:Type Path}">
        <Setter Property="Fill" Value="{{colors.color8}}" />
    </Style>
    <Style x:Key="ClockBox" BasedOn="{StaticResource BaseClockBox}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.color8}}" />
    </Style>
    <Style x:Key="DateBox" BasedOn="{StaticResource BaseDateBox}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.color8}}" />
    </Style>
    <Style x:Key="PreviewBorderStyle" BasedOn="{StaticResource BasePreviewBorderStyle}" TargetType="{x:Type Border}">
        <Setter Property="BorderBrush" Value="{{colors.color0}}" />
    </Style>
    <Style x:Key="PreviewItemTitleStyle" BasedOn="{StaticResource BasePreviewItemTitleStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.foreground}}" />
    </Style>
    <Style x:Key="PreviewItemSubTitleStyle" BasedOn="{StaticResource BasePreviewItemSubTitleStyle}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.color8}}" />
    </Style>
    <Style x:Key="PreviewGlyph" BasedOn="{StaticResource BasePreviewGlyph}" TargetType="{x:Type TextBlock}">
        <Setter Property="Foreground" Value="{{colors.foreground}}" />
    </Style>
</ResourceDictionary>
